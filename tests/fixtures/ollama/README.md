# Captured Ollama streams

Captured from Ollama 0.35.1 using the frozen `explicit` Spanish fixture (10 × 20 × 30 mm). These files contain generated test text and protocol metadata; no account credentials.

- `qwen3-explicit.ndjson`: local Qwen3 4B Q4_K_M, manifest digest `359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7`, from the initial CPU/schema baseline. This individual response is valid; the overall baseline failed at 12/20.
- `kimi-cloud-explicit.ndjson`: explicitly selected `kimi-k2.7-code:cloud`, official remote host `https://ollama.com:443`, from the 20/20 cloud benchmark. Ollama emits the remote model name and omits per-record routing metadata; the stream was labeled `application/json`. Profile identity/routing is verified before prompt transmission. The cloud manifest pins routing metadata, not immutable remote weights.

The regular Rust regression test replays both recordings at every possible byte boundary. Synthetic fixtures separately exercise accented UTF-8, errors, truncation, duplicates, bounds, and cancellation.
