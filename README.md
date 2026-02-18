# First Officer

Use GitHub Copilot as if it was the Anthropic API (e.g. for Claude Code).

This was inspired by <https://github.com/ericc-ch/copilot-api> but in Rust with a different feature set to make it easier to use with things that expect the Claude Code / Anthropic API.

Compared to the original, there's also no persistent state, so it can be scaled, and you can use multiple github tokens (ie it can be used by multiple people).
See below for more features.

## Quick start

Start the server:

```console
$ docker run -d --rm -p 4141:4141 ghcr.io/passcod/first-officer:latest
```

Grab a GitHub Copilot token:

```console
$ npx copilot-api@latest auth
$ cat ~/.local/share/copilot-api/github_token
```

Use the token to authenticate with the server:

```console
$ env ANTHROPIC_BASE_URL="http://localhost:4141" ANTHROPIC_AUTH_TOKEN="ghp_..." claude
```

## Configuration

Entirely done by environment variables, all of them optional:

- `PORT`: The port to listen on. Defaults to `4141`.
- `RUST_LOG`: The log level.
- `VSCODE_VERSION`: The version of VS Code we're pretending to be. Defaults to `1.100.0`.
- `ACCOUNT_TYPE`: Set to `business` or `enterprise` if using those GitHub account types.

## Authentication Token

To use the API, you need a GitHub token. See how to get one in the Quick Start section.

You can either:

- use the token as an Anthropic API key (or in the OpenAI convention, that works too);
- set the `GH_TOKEN` environment variable on the server, and use any value (e.g. `-`) as the API key.

If you don't provide a valid token one way or another, you'll get a 403 response.
If you provide a token as an API key and the `GH_TOKEN` variable was set, the API key will be preferred (it acts as a fallback).
That way you can have the service work for multiple people with independent tokens.

## Model renaming

The way GitHub names its models is not quite how Anthropic names its models.
You _can_ configure Claude Code or whatever to use the right models using its config file, but then you need to always keep that updated.

First Officer instead dynamically translates model names between how they are in the Copilot API to a naming scheme like Claude Code expects.
This means it will also work with other software that expects the Anthropic model names.
It's not a hard-coded mapping, so when new models become available in Copilot, this software usually doesn't need an update.

There's also two environment variables available to further customise this:

- `MODEL_RENAME_AUTO` — set to `false` to disable pattern-based auto renaming.
- `MODEL_RENAME_MAP` — JSON object `{"copilot-name": "api-name", ...}` applied on top of auto rules (custom entries take priority).

We also strip date-pinned model names, so if something requests `claude-sonnet-4-5-20250115` we'll just serve `claude-sonnet-4-5`.

## Thinking emulation

The Copilot API doesn't support Anthropic's "thinking" mode, but First Officer emulates it.
You can disable this by setting `EMULATE_THINKING` to `false`.

## Model list cache

The `/v1/models` route returns the list of available models, as expected in the OpenAI API.
However, to save on API calls, that list is cached, with an default TTL of 1 hour.
You can change that with the `MODELS_CACHE_TTL` and an integer value in seconds.
Set to 0 to disable caching.
