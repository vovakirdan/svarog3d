# TODO: 
# 1. Создать команду build для сборки workspace через cargo.
# 2. Создать команду run для запуска бинаря app через cargo.
# 3. Создать команду run-wsl для запуска app с отключённым WAYLAND_DISPLAY (для WSL/CI).

.PHONY: build, run, run-wayland-off
build:
	@echo "Building all workspace crates..."
	cargo build --workspace

run:
	@echo "Running app crate..."
	cargo run -p app

run-wayland-off:
	@echo "Running app crate with WAYLAND_DISPLAY unset (WSL/CI)..."
	env -u WAYLAND_DISPLAY cargo run -p app

run-gpu-vulkan:
	@echo "Running app crate with GPU backend VK..."
	cargo run -p app -- --gpu-backend=vulkan

run-gpu-gl:
	@echo "Running app crate with GPU backend GL..."
	cargo run -p app -- --gpu-backend=gl

run-gpu-dx12:
	@echo "Running app crate with GPU backend DX12..."
	cargo run -p app -- --gpu-backend=dx12

run-gpu-metal:
	@echo "Running app crate with GPU backend Metal..."
	cargo run -p app -- --gpu-backend=metal
