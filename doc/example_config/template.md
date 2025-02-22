---
title: "#{{ timestamp | dateformat(format='[week_number]') }}: This Week in X"
author: {{ editor }}
date: {{ timestamp | datetimeformat(format="iso") }}
tags: {{ projects }}
categories: ["weekly-update"]
draft: false
---
{#-
 # Here are some pointers to get started writing custom templates:
 #
 # - This template is processed using MiniJinja:
 #   https://docs.rs/minijinja/latest/minijinja/
 #
 # - Template syntax is mostly compatible with Jinja2:
 #   https://jinja.palletsprojects.com/en/3.1.x/templates/
 #
 # - Date formatting is done using time.rs format specifiers:
 #   https://time-rs.github.io/book/api/format-description.html
 #
 # - Adding {{ debug() }} will insert the contents of the environment
 #   used for processing the template. This is useful for writing custom
 #   templates.
 #
 # - On top of the MiniJinja built-ins (including the minijinja-contrib ones),
 #   the following globals are available during template processing:
 #
 #     editor: display name of the editor that issued the !render command.
 #     timestamp: date/time of the !render command (see more below).
 #     sections: sections containing projects and news items.
 #     projects: names of the projects that have news items.
 #     config: contents of the TOML configuration file.
 #
 # - The "timestamp" global contains the date and time at which the !render
 #   command was issued. The timedelta() filter can be used to derive new
 #   date/time values by adding and subtracting time periods, e.g.:
 #   "now() | timedelta(weeks=-1)" obtains the date/time for one week
 #   ago. Accepted parameters are "seconds", "minutes", "hours", "days",
 #   "weeks", "months", and "years".
 #
 # - Macros can be used to avoid repeating template fragments. See below
 #   for an example macro to handle both section and project news.
 #
 # - Hebbot will detect when the template has changed on disk and reload
 #   the file contents the next time it receives a !render command.
-#}

{%- macro news(news_items) -%}
  {%- for item in news_items %}

[{{ item.reporter_display_name }}](https://matrix.to/#{{ item.reporter_id }}) {{ config.verbs | random }}

> {{ item.message | replace("\n", "\n> ") }}
    {%- if item.images -%}
      {%- for image in item.images %}
> ![]({{ image[0] }})
      {%- endfor %}
    {%- endif -%} {#- news item images #}

    {%- if item.videos -%}
      {%- for video in item.videos %}
> {{ "{{" }}<video src="{{ video[0] }}">{{ "}}" }}
      {%- endfor %}
    {%- endif -%} {#- news item videos #}

  {%- endfor %} {#- news_items #}
{%- endmacro %}

Update on what happened across the X project in the week from {{
  timestamp | timedelta(weeks = -1) | dateformat(format="[month repr:long] [day padding:none]")
}} to {{
  timestamp | dateformat(format="[month repr:long] [day padding:none]") }}.

{%- for key, entry in sections | dictsort %}

## {{entry.section.title}} {{entry.section.emoji}}

  {{- news(entry.news) }}

  {%- for entry in entry.projects %}

### {{ entry.project.title}} [↗]({{ entry.project.website }}) {{ entry.project.emoji }}

{{ entry.project.description }}

    {{- news(entry.news) }}

  {%- endfor %} {#- projects #}

{%- endfor %} {#- sections #}

# That’s all for this week!

See you next week!
