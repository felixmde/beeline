# Beeline

Beeline is a command-line tool to interact with Beeminder. 

Like [bmndr](https://github.com/lydgate/bmndr), only with less features. Also,
why use a 120 loc Python script if you can use a Rust tool with 200
dependencies?

## Installation

Clone the repository and run the tool using Cargo:

```bash
git clone <repository-url>
cd beeline
cargo run
```

Before using Beeline, ensure your Beeminder API key is set via an environment
variable:

```bash
export BEEMINDER_API_KEY='your_api_key_here'
```

## Usage

To list all active Beeminder goals:

```bash
cargo run -- list
```

To add a datapoint to a specific goal:

```bash
cargo run -- add <goal_slug> <datapoint_value> [<comment_text>]
```

For example, to add a datapoint to a goal called 'fitness':

```bash
cargo run -- add fitness 1.0 "worked out today"
```

## Contributing

This tool is currently tailored to my personal needs. However, if you find it
useful and would like to contribute, please feel free to submit a pull request
or open an issue. Together, we can enhance the tool into something more broadly
useful.
