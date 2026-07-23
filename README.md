# AI assistant

A general-purpose personal AI assistant, including for coding.

---

##  Table of Contents

- [Installation](#installation)
- [Usage](#usage)
  - [Examples](#examples)
- [License](#license)

---

## Installation

`assistant` uses Cargo, so you need a recent Rust toolchain installed.

```bash
cargo install --git https://github.com/silvafass/assistant
```

---

## Usage

```bash
Usage: assist [OPTIONS]
       assist <COMMAND>

Commands:
  general  Run in general-purpose mode [Default mode]
  coding   Run in coding-purpose mode
  acp      Run in integration-purpose mode via Agent Client Protocal server (Code editor integrations)
  help     Print this message or the help of the given subcommand(s)

Options:
  -m, --model <MODEL>
          Model name

          [default: gemma4]

  -i, --input <INPUT>
          To receive the prompt

  -c, --compatibility <COMPATIBILITY>
          The API compatibility to use

          Possible values:
          - ollama:     Ollama API compatibility
          - open-ai:    OpenAI API compatibility (useful for integrate with OpenAI API-compatible providers)
          - mistral-rs: Mistral-rs integration compatibility (conveniently runs as an OpenAI API-compatible provider)

          [default: ollama]

  -a, --api-base-url <API_BASE_URL>
          Provider API base URL

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### Examples

#### 1. Simple prompt

**Via --input option**
```bash
assist --input "Hello!"
```
> Hello! How can I help you today? 😊

**Via stdio**
```bash
echo "Hello!" | assist
```
> Hello! How can I help you today? 😊

#### 2. REPL interation

```bash
assist
```
> ❯ Hello!
>
> Hello! How can I help you today?
>
> ❯ Say something funny
>
> Why did the scarecrow win an award?
>
>
> ...Because he was outstanding in his field! 😂
>
> ❯ /help
>
> Commands:
>
>   /help  Print help
>
>   /quit  Exit
>
> ❯

---

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
