default:
    just --list

dev:
    cd daemon && cargo run

web:
    cd web && pnpm dev

build:
    cd web && pnpm build
    cd daemon && cargo build --release
