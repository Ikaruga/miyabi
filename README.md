# test_egui_chat

Mini client de chat desktop en Rust pour LM Studio, avec support tool calling et mémoire persistante.

## Features

- Interface egui (Rust, zéro HTML)
- Streaming LM Studio (endpoint OpenAI-compatible)
- Tool calling : `list_dir`, `read_file`, `write_file`, `make_dir`, `edit_file`, `run_command`
- Mémoire vectorielle persistante (knowledge.db) : `save_knowledge`, `search_knowledge`, `list_knowledge`, `delete_knowledge`
- Embeddings via nomic-embed-text (768 dims, cosine similarity pur Rust)
- Sandbox workdir avec modes de permission (read-only, restricted, full)
- Thought Flow panel (raisonnement structuré visible)
- Pattern Cycle Agent (task_state.db) en fondations (v8+)

## Prérequis

- [Rust](https://rustup.rs/) stable
- [LM Studio](https://lmstudio.ai/) avec un modèle chargé (testé sur `qwen/qwen3.5-9b` et équivalents) + `text-embedding-nomic-embed-text-v1.5` pour la mémoire

## Build

```bash
cargo build --release
./target/release/test_egui_chat.exe
```

## Tests

```bash
cargo test --release
```

3 tests unitaires inclus (filtrage workdir, schémas DB).

## Configuration

Créer un `system_prompt.txt` à la racine pour la guidance du modèle (non versionné).

## Licence

TBD
