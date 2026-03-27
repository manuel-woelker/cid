# What is this document for?

This document describes the intended daemon-level configuration file for `cid`.
It focuses on configuring which local Git repositories the daemon should manage.

This is not the per-repository pipeline format.
It is the daemon’s own configuration for which repositories exist in its local world.

# What should the daemon config file be called?

The intended filename is:

```text
cid-config.yaml
```

YAML is a good fit here because the structure is small, familiar, and easy to extend later without inventing a custom syntax.

# What should the first version configure?

The first version should configure only one thing:

- the list of repositories the daemon should watch

That is enough to get the daemon off the ground without smuggling pipeline logic, scheduling rules, and half a CI DSL into one file.

# What should the first version look like?

The recommended initial shape is:

```yaml
repositories:
  - path: /home/user/foo
  - path: /home/user/bar
```

Each entry is one explicitly registered local Git repository.

# Why use `repositories` instead of `watch.directories`?

`repositories` is the better MVP shape because it is explicit and predictable.

It tells `cid` exactly which repositories exist, instead of asking it to discover repositories under broader directory roots.

That avoids a lot of awkward questions early on, such as:

- should nested repositories be included?
- what about unrelated Git repos under the same tree?
- should newly created repos be discovered automatically?
- how should exclusions work?

Those are real questions, but they are not useful first-version problems.

# What does each repository entry mean?

Each repository entry declares one local repository path that `cid` should manage.

For the first version, `path` is enough.

Example:

```yaml
repositories:
  - path: /home/user/foo
```

That means:

- `cid` should treat `/home/user/foo` as a managed repository
- the path should point at a local Git working tree
- any additional repository metadata should be derived or stored elsewhere until the schema grows

# How should this format evolve later?

The list-item object shape is intentional.
It gives the config room to grow without changing the top-level structure.

For example, a later version could extend entries like this:

```yaml
repositories:
  - path: /home/user/foo
    enabled: true
    branch_patterns:
      - main
      - feature/*
```

That is cleaner than starting with a list of raw strings and then needing a breaking format change later.

# What should stay out of this file?

This daemon config should stay focused on daemon concerns.

At least initially, do not put these things in `cid-config.yaml`:

- pipeline step definitions
- Docker image commands for builds
- artifact declarations
- scheduling policy experiments
- dashboard preferences

Those belong in separate config surfaces if and when they are needed.

# What should the daemon validate?

At load time, the daemon should validate at least:

- the file parses as YAML
- `repositories` is present and is a list
- every entry contains a `path`
- each `path` exists
- each `path` points to a Git repository

The error messages should be blunt and readable.
Broken config should fail loudly instead of being “helpfully” guessed into something else.
