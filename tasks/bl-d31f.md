+++
title = "bl create/update: honor a `--` end-of-options separator so callers can pass untrusted positionals"
created = 1780996671
updated = 1780996671
priority = 4
tags = ["core", "from-gh-plugin"]
+++
Surfaced by the gh-plugin satellite trial (bl-8f5b). src/mutate.rs parse() has no `--` arm: a literal `--` falls into `flag if flag.starts_with('-') => Err(unexpected flag)`. So a plugin shelling `bl create <title>` with an untrusted (GitHub-sourced) title cannot guard it — a `-`-leading title hijacks a known flag (bounded: trailing --as is last-wins, no shell, so low severity — mistitle/DoS of one issue, no escalation). Fix: add a `"--" => rest-are-positionals` arm to parse() (create+update), then callers append `-- <title>`. Low priority; enables the plugin-side guard bl-8f5b.