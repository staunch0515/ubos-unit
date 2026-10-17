---
id: UBS-SYS-WSP-06
title: Workspace — Configuration
status: draft
phase: PH-2
depends_on: [UBS-SYS-WSP-02]
---

# Workspace — Configuration

| Setting | Level | Default | Design |
|---|---|---|---|
| branding (logo, colours, fonts) | tenant | UBOS theme (design system 1.0, DSN-WSP-211) | DSN-WSP-206 |
| default locale and time-zone display | tenant, user | en-US, business time zone | DSN-WSP-207 |
| theme and table density | user | system theme, compact | DSN-WSP-211, DSN-WSP-213 |
| negative amounts | user | parentheses | DSN-WSP-120, DSN-WSP-213 |
| layer vocabulary list | platform | built-in | DSN-WSP-114 |
| draft autosave interval | platform | 5 s | DSN-WSP-105 |
| as-of banner colour | tenant | amber | DSN-WSP-108 |
| portal enabled and domain | tenant | off | DSN-WSP-205 |
| signature authentication level per document class | tenant (business data) | email + OTP | DSN-WSP-501 |
| feature flags (Smart Grid, War Room, Books) | plan | per plan | DSN-WSP-401 |
