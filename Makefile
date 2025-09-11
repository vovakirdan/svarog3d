# TODO: 
# 1. Создать команду build для сборки workspace через cargo.
# 2. Создать команду run для запуска бинаря app через cargo.
# 3. Создать команду run-wsl для запуска app с отключённым WAYLAND_DISPLAY (для WSL/CI).

.PHONY: build
build:
	@echo "Building all workspace crates..."
	cargo build --workspace

.PHONY: run
run:
	@echo "Running app crate..."
	cargo run -p app

.PHONY: run-wsl
run-wsl:
	@echo "Running app crate with WAYLAND_DISPLAY unset (WSL/CI)..."
	env -u WAYLAND_DISPLAY cargo run -p app
