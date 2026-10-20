# AI Integration (Ollama)

StockWise can add plain-English commentary from a **local** model via
[Ollama](https://ollama.com). Inference runs on your machine — no stock data,
portfolio, or credentials are sent to an external AI service. Everything else
in StockWise works without it.

## Setup

```bash
# Install Ollama (macOS; see ollama.com for Linux/Windows)
brew install ollama

# Start the server
ollama serve

# Pull the default model
ollama pull qwen3:14b

# Check that StockWise can see it
stockwise doctor
```

## Two ways to use it

**Dedicated reports** always use the model:

```bash
stockwise report intraday     # Morning trading brief
stockwise report longterm     # Investment memo for SIP
stockwise report portfolio    # Portfolio health review
stockwise report RELIANCE     # Single-stock analysis
```

**Opt-in sections** on analysis commands — add `--ai` (or set `STOCKWISE_AI=1`
to make it the default):

```bash
stockwise analyze RELIANCE --ai
stockwise compare TCS INFY --ai
```

| Command | AI section |
|---------|-----------|
| `analyze` | Fundamental verdict |
| `technical` | Indicator confluence interpretation |
| `compare` | "Which looks stronger?" verdict |
| `backtest` | Strategy viability notes |
| `screen` | Notes on the screened names, value-trap warnings |
| `deep` | Summary of the full report |

AI is opt-in because a local model typically takes **30–60 seconds** per
answer, which would turn these sub-second commands into minute-long waits.
StockWise prints a progress line while the model works, and caches answers for
an hour when Redis is available.

## Configuration

| Variable | Default | Purpose |
|----------|---------|---------|
| `STOCKWISE_MODEL` | `qwen3:14b` | Model name as shown by `ollama list` |
| `OLLAMA_HOST` | `http://localhost:11434` | Ollama server (`host:port` or a full URL) |
| `STOCKWISE_AI` | off | `1` to add AI sections to analysis commands by default |

```bash
STOCKWISE_MODEL=llama3.1:8b stockwise report portfolio
OLLAMA_HOST=192.168.1.20:11434 stockwise report TCS
```

If the model isn't pulled, StockWise tells you the exact `ollama pull` command
instead of printing an empty section.

## Suggested models

| Model | Download | Notes |
|-------|----------|-------|
| `qwen3:14b` | ~9 GB | Default. Strong structured analysis |
| `gemma3:12b` | ~8 GB | Good general reasoning |
| `phi4:14b` | ~9 GB | Good with financial metrics |
| `llama3.1:8b` | ~5 GB | Lighter and faster |

Model output is commentary, not advice. See [DISCLAIMER.md](../DISCLAIMER.md).
