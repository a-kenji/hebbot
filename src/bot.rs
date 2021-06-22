use matrix_sdk::events::{room::message::MessageEventContent, AnyMessageEventContent};
use matrix_sdk::room::Joined;
use matrix_sdk::room::Room;
use matrix_sdk::Client;
use matrix_sdk::EventHandler;
use matrix_sdk::RoomMember;
use matrix_sdk::SyncSettings;
use matrix_sdk_common::uuid::Uuid;
use ruma::events::reaction::ReactionEventContent;
use ruma::events::room::redaction::SyncRedactionEvent;
use ruma::events::SyncMessageEvent;
use ruma::EventId;
use ruma::RoomId;
use ruma::UserId;

use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::Mutex;

use crate::config::Config;
use crate::render;
use crate::store::{News, NewsStore};
use crate::utils;

#[derive(Clone)]
pub struct Bot {
    config: Config,
    news_store: Arc<Mutex<NewsStore>>,
    client: Client,
    reporting_room: Joined,
    admin_room: Joined,
}

impl Bot {
    pub async fn run() {
        let config = Config::read();
        let news_store = Arc::new(Mutex::new(NewsStore::read()));

        let username = config.bot_user_id.as_str();
        let user = UserId::try_from(username).expect("Unable to parse bot user id");
        let client = Client::new_from_user_id(user.clone()).await.unwrap();

        Self::login(&client, user.localpart(), &config.bot_password).await;

        // Get matrix rooms
        let reporting_room_id = RoomId::try_from(config.reporting_room_id.as_str()).unwrap();
        let reporting_room = client
            .get_joined_room(&reporting_room_id)
            .expect("Unable to get reporting room");

        let admin_room_id = RoomId::try_from(config.admin_room_id.as_str()).unwrap();
        let admin_room = client
            .get_joined_room(&admin_room_id)
            .expect("Unable to get admin room");

        let bot = Self {
            config,
            news_store,
            client,
            reporting_room,
            admin_room,
        };

        //bot.send_message("Started hebbot service!", true).await;
        let handler = Box::new(EventCallback(bot.clone()));
        bot.client.set_event_handler(handler).await;

        info!("Start syncing...");
        bot.client.sync(SyncSettings::new()).await;
    }

    async fn login(client: &Client, user: &str, pwd: &str) {
        info!("Logging in...");
        let response = client
            .login(user, pwd, None, Some("hebbot"))
            .await
            .expect("Unable to login");

        info!("Do initial sync...");
        client
            .sync_once(SyncSettings::new())
            .await
            .expect("Unable to sync");

        info!(
            "Logged in as {}, got device_id {} and access_token {}",
            response.user_id, response.device_id, response.access_token
        );
    }

    async fn send_message(&self, msg: &str, html: bool, admin_room: bool) {
        let content = if html {
            AnyMessageEventContent::RoomMessage(MessageEventContent::text_html(msg, msg))
        } else {
            AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(msg))
        };
        let txn_id = Uuid::new_v4();

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content, Some(txn_id))
            .await
            .expect("Unable to send message");
    }
}

struct EventCallback(Bot);

#[async_trait::async_trait]
impl EventHandler for EventCallback {
    async fn on_room_message(&self, room: Room, event: &SyncMessageEvent<MessageEventContent>) {
        if let Room::Joined(ref _joined) = room {
            // Standard text message
            if let Some(text) = utils::get_message_event_text(event) {
                let member = room.get_member(&event.sender).await.unwrap().unwrap();

                // Reporting room
                if room.room_id() == self.0.reporting_room.room_id() {
                    let id = &event.event_id;
                    self.on_reporting_room_msg(text.clone(), &member, id).await;
                }

                // Admin room
                if room.room_id() == self.0.admin_room.room_id() {
                    self.on_admin_room_message(text, &member).await;
                }
            }
        }
    }

    async fn on_room_reaction(&self, room: Room, event: &SyncMessageEvent<ReactionEventContent>) {
        //dbg!(&event);
        if let Room::Joined(ref _joined) = room {
            let relation = &event.content.relation;
            let reaction_event_id = event.event_id.clone();
            let message_event_id = relation.event_id.clone();
            let reaction_sender = room.get_member(&event.sender).await.unwrap().unwrap();

            // Reporting room
            if room.room_id() == self.0.reporting_room.room_id() {
                let emoji = &relation.emoji;
                // Remove emoji variant form
                let emoji = emoji.replace("\u{fe0f}", "");

                self.on_reporting_room_reaction(
                    &reaction_sender,
                    &emoji,
                    &message_event_id,
                    &reaction_event_id,
                )
                .await;
            }
        }
    }

    async fn on_room_redaction(&self, room: Room, event: &SyncRedactionEvent) {
        //dbg!(&event);
        if let Room::Joined(ref _joined) = room {
            let redacted_event_id = event.redacts.clone();
            let member = room.get_member(&event.sender).await.unwrap().unwrap();

            // Reporting room
            if room.room_id() == self.0.reporting_room.room_id() {
                self.on_reporting_room_redaction(&member, &redacted_event_id)
                    .await;
            }
        }
    }
}

impl EventCallback {
    async fn on_reporting_room_msg(
        &self,
        message: String,
        member: &RoomMember,
        event_id: &EventId,
    ) {
        // We're going to ignore all messages, expect it mentions the bot at the beginning
        let bot_id = self.0.client.user_id().await.unwrap();
        if !utils::msg_starts_with_mention(bot_id, message.clone()) {
            return;
        }

        let event_id = event_id.to_string();
        let reporter_id = member.user_id().to_string();
        let reporter_display_name = utils::get_member_display_name(&member);

        let msg = format!(
            "Thanks for the report {}, I'll store your update!",
            reporter_display_name
        );
        self.0.send_message(&msg, false, false).await;

        let news = News {
            event_id,
            reporter_id,
            reporter_display_name,
            message,
            ..Default::default()
        };

        self.0.news_store.lock().unwrap().add_news(news);
    }

    async fn on_reporting_room_reaction(
        &self,
        reaction_sender: &RoomMember,
        reaction_emoji: &str,
        message_event_id: &EventId,
        reaction_event_id: &EventId,
    ) {
        // Check if the sender is a editor (= has the permission to use emoji commands)
        if !self.is_editor(&reaction_sender).await {
            return;
        }

        let approval_emoji = &self.0.config.approval_emoji;
        if reaction_emoji == approval_emoji.to_string() {
            let message_event_id = message_event_id.to_string();
            let reaction_event_id = reaction_event_id.to_string();

            let msg = {
                let mut news_store = self.0.news_store.lock().unwrap();
                match news_store.approve_news(&message_event_id, &reaction_event_id) {
                    Ok(news) => format!(
                        "Editor {} approved {}'s news entry (ID {})",
                        reaction_sender.user_id().to_string(),
                        news.reporter_id,
                        message_event_id
                    ),
                    Err(err) => format!(
                        "Unable to add {}'s news approval (ID {}): {:?}",
                        reaction_sender.user_id().to_string(),
                        message_event_id,
                        err
                    ),
                }
            };
            self.0.send_message(&msg, false, true).await;
        } else {
            debug!(
                "Ignore emoji reaction, doesn't match approval emoji (approval: {:?} vs. reaction: {:?})",
                approval_emoji, reaction_emoji
            );
        }
    }

    async fn on_reporting_room_redaction(&self, member: &RoomMember, redacted_event_id: &EventId) {
        // Check if the sender is a editor (= has the permission to use emoji commands)
        if !self.is_editor(&member).await {
            return;
        }

        let msg = {
            let mut news_store = self.0.news_store.lock().unwrap();
            if let Ok(news) = news_store.unapprove_news(&redacted_event_id.to_string()) {
                let mut msg = format!(
                    "Editor {} removed their approval from {}'s news entry (ID {}).",
                    member.user_id().to_string(),
                    news.reporter_id,
                    news.event_id
                );

                if news.approvals.is_empty() {
                    msg += " This news entry doesn't have an approval anymore."
                }

                Some(msg)
            } else {
                None
            }
        };

        if let Some(msg) = msg {
            self.0.send_message(&msg, false, true).await;
        }
    }

    async fn on_admin_room_message(&self, msg: String, member: &RoomMember) {
        // Check if the message is a command
        if !msg.as_str().starts_with('!') {
            return;
        }

        // Check if the sender is a editor (= has the permission to use commands)
        if !self.is_editor(&member).await {
            let msg = "You don't have the permission to use commands.";
            self.0.send_message(msg, false, true).await;
            return;
        }

        // Parse command and optional args
        let mut split: Vec<&str> = msg.splitn(2, ' ').collect();
        let args = if split.len() == 2 {
            split.pop().unwrap()
        } else {
            ""
        };
        let command = split.pop().unwrap_or("");

        info!("Received command: {} ({})", command, args);

        match command {
            "!render-message" => self.render_message_command(member).await,
            "!status" => self.status_command().await,
            "!clear" => self.clear_command().await,
            "!help" => self.help_command().await,
            "!say" => self.say_command(&args).await,
            _ => self.unrecognized_command().await,
        }
    }

    async fn help_command(&self) {
        let help = "Available commands: \n\n\
            !render-message \n\
            !render-file \n\
            !status \n\
            !clear \n\
            !say <message>";

        self.0.send_message(help, false, true).await;
    }

    async fn status_command(&self) {
        let msg = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.get_news();

            let news_count = news.len();
            let mut news_approved_count = 0;

            for n in news {
                if !n.approvals.is_empty() {
                    news_approved_count += 1;
                }
            }

            format!(
                "Status: \n\n\
                All news: {} \n\
                Approved news: {}",
                news_count, news_approved_count
            )
        };

        self.0.send_message(&msg, false, true).await;
    }

    async fn render_message_command(&self, editor: &RoomMember) {
        let rendered = {
            let bot = self.0.client.user_id().await.unwrap();

            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.get_news();

            let r = render::render(news, editor, &bot);

            format!("<pre><code>{}</code></pre>\n", r)
        };

        self.0.send_message(&rendered, true, true).await;
    }

    async fn clear_command(&self) {
        let msg = {
            let mut news_store = self.0.news_store.lock().unwrap();

            let news = news_store.get_news();
            news_store.clear_news();

            format!("Cleared {} news!", news.len())
        };

        self.0.send_message(&msg, false, true).await;
    }

    async fn say_command(&self, msg: &str) {
        self.0.send_message(&msg, true, false).await;
    }

    async fn unrecognized_command(&self) {
        let msg = "Unrecognized command. Use !help to list available commands.";
        self.0.send_message(msg, false, true).await;
    }

    async fn is_editor(&self, member: &RoomMember) -> bool {
        let user_id = member.user_id().to_string();
        self.0.config.editors.contains(&user_id)
    }
}
