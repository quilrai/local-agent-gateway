# Quilr local agent gateway

https://quilrbusiness.github.io/local-agent-gateway/

Desktop App for monitoring and controlling llm requests (with focus on coding agents)

## Features

- Fully local (on-device), desktop app
- Pass through proxy server for llm requests
- Block or redact requests with sensitive information (pre-defined patterns and user defined patterns)
- Warn or Block for high token count requests
- Customizable rate limiting
- Searchable Full request log with response

## How it works

- The app starts a local pass through server
- claude code, codex and many other coding agents support customizable base url
- with base url set for these agents, requests will be passed through the local server running in the app
- patterns, blocking etc settings are applied (if configured) in the app

**cursor** Cursor does not provide a way to monitor / control data via network. For cursor, integration with hooks is implemented. note that cursor does not support auto redaction, exposing exact token counts etc

**Custom LLM endpoints**
- In the app, you can configure a custom chat completions endpoint
- This feature is useful if you are using your own token with a LLM endpoint, and you want to monitor / control data

## Detections

- Block or Redact data going to LLMs automatically with intelligent pattern matching
- Pre-defined patterns for general use cases (API Keys, credentials, etc)
