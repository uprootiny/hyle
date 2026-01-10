# Free Model Evaluation Results

*Last updated: 2025-01-10*

## Summary

hyle includes a batch model evaluation mode (`hyle eval`) that tests free OpenRouter models on coding tasks. This document captures the findings.

## Evaluation Methodology

Models are tested on 4 tasks:
1. **rust_fn**: Write a palindrome checker function
2. **explain_code**: Explain a recursive Fibonacci implementation
3. **bug_fix**: Fix a Rust borrow issue
4. **tool_use**: Correctly format a tool call

Each response is scored on:
- **Coherence**: Makes grammatical/logical sense
- **Completeness**: Answers the question
- **Tool validity**: Correct tool call format
- **Code quality**: Balanced braces, valid syntax
- **Relevance**: On-topic response

## Results (2025-01-10)

### A-Grade (Recommended)

| Model | Quality | Latency | Notes |
|-------|---------|---------|-------|
| `mistralai/devstral-2512:free` | 79% | 1.5s | **Best value** - fast + quality |
| `nvidia/nemotron-3-nano-30b-a3b:free` | 76% | 1.8s | Fast, reliable |
| `tngtech/deepseek-r1t-chimera:free` | 80% | 15s | Highest quality, slow |
| `deepseek/deepseek-r1-0528:free` | 77% | 27s | Good quality, very slow |

### B-Grade (Usable with caveats)

| Model | Quality | Latency | Notes |
|-------|---------|---------|-------|
| `qwen/qwen3-coder:free` | 75% | 4s | Rate-limits easily after 2-3 requests |

### Avoid (Known issues)

| Model | Issue |
|-------|-------|
| `nvidia/nemotron-nano-9b-v2:free` | Privacy policy error (404) |
| `google/gemma-3n-e4b-it:free` | "Developer instruction not enabled" |
| `moonshotai/kimi-k2:free` | Privacy policy error (404) |

## Recommendations

### For Interactive Use (`hyle --free`)
Use **devstral-2512** - best balance of quality and speed.

### For Background Tasks (`hyle --backburner`)
Use **deepseek-r1t-chimera** - highest quality, latency doesn't matter.

### For High-Throughput
Use **nemotron-3-nano-30b-a3b** - fast, reliable, good quality.

## Running Your Own Evaluation

```bash
# Test default recommended models
hyle eval

# Test specific models
hyle eval "mistralai/devstral-2512:free" "nvidia/nemotron-3-nano-30b-a3b:free"

# Custom task
hyle eval --task "Write a Rust function that reverses a string"

# Save results to JSON
hyle eval --output results.json
```

## Rate Limit Handling

Free models have aggressive rate limits. hyle handles this by:
1. Detecting 429 responses
2. Exponential backoff (2s, 4s, 8s)
3. Up to 3 retry attempts
4. Automatic model switching when a model fails repeatedly

## Model Rotation

For sustained use, hyle automatically rotates through fallback models when rate limits are hit. The rotation order matches the quality ranking above.
