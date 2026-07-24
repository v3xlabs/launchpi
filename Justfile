default:
    just --list

dev:
    cd daemon && cargo run

web:
    cd web && pnpm dev
