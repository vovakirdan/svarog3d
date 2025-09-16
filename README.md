# Svarog3D

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.8-blue.svg)](Cargo.toml)

**Svarog3D** — современный 3D движок на Rust, построенный на основе wgpu для кроссплатформенной графики. Проект реализует модульную архитектуру с фокусом на производительность, безопасность типов и минимальные зависимости.

## ✨ Особенности

- 🔥 **Современная графика**: Использует wgpu для поддержки Vulkan, DirectX 12, Metal и OpenGL
- ⚡ **Высокая производительность**: Оптимизированный рендеринг с батчингом и инстансингом
- 🏗️ **Модульная архитектура**: Разделение на независимые крейты для лучшей организации кода
- 🎮 **ECS система**: Легковесная Entity-Component-System для управления сценой
- 🔧 **Гибкая настройка**: Поддержка различных графических бэкендов через CLI
- 📦 **Минимальные зависимости**: Осознанный выбор зависимостей для быстрой сборки

## 🏛️ Архитектура

Проект организован в виде workspace с пятью основными крейтами:

```
svarog3d/
├── crates/
│   ├── app/         # Точка входа и CLI интерфейс
│   ├── platform/    # Оконная система и event loop (winit)
│   ├── renderer/    # Рендеринг на wgpu, шейдеры, пайплайны
│   ├── corelib/     # Математика, ECS, базовые типы
│   └── asset/       # Система ресурсов (в разработке)
└── Makefile         # Удобные команды для сборки и запуска
```

### Зависимости между крейтами

```
app → platform → renderer → corelib
                     ↓
                  asset (планируется)
```

### Основные компоненты

- **`corelib`**: Базовая математика (glam), Transform, Camera, мини-ECS
- **`renderer`**: wgpu инициализация, вершинные буферы, шейдеры WGSL, depth buffer
- **`platform`**: Управление окном через winit, event loop, интеграция с renderer
- **`app`**: CLI парсинг, логирование, точка входа приложения

## 🚀 Быстрый старт

### Требования

- Rust 2024 edition (1.75+)
- Совместимая графическая карта с поддержкой Vulkan/DirectX/Metal/OpenGL

### Сборка и запуск

```bash
# Клонирование репозитория
git clone <repository-url>
cd svarog3d

# Сборка всех крейтов
make build
# или
cargo build --workspace

# Запуск с настройками по умолчанию
make run
# или
cargo run -p app
```

### Параметры командной строки

```bash
# Выбор графического бэкенда
cargo run -p app -- --gpu-backend=vulkan  # Vulkan
cargo run -p app -- --gpu-backend=dx12    # DirectX 12
cargo run -p app -- --gpu-backend=metal   # Metal (macOS)
cargo run -p app -- --gpu-backend=gl      # OpenGL

# Настройка размера окна
cargo run -p app -- --size=1920x1080
cargo run -p app -- --width=1600 --height=900

# Отображение FPS
cargo run -p app -- --show-fps

# Комбинирование параметров
cargo run -p app -- --gpu-backend=vulkan --size=1920x1080 --show-fps
```

### Makefile команды

```bash
make help                # Показать все доступные команды
make build              # Сборка workspace
make run                # Запуск приложения
make run-wayland-off    # Запуск в WSL/CI (без Wayland)
make run-gpu-vulkan     # Запуск с Vulkan
make run-gpu-gl         # Запуск с OpenGL
make run-gpu-dx12       # Запуск с DirectX 12
make run-gpu-metal      # Запуск с Metal
```

## 🎯 Текущее состояние

### Реализовано ✅

- **A1-A2**: Оконная система с winit, логирование, CLI параметры
- **B1-B3**: wgpu инициализация, базовые шейдеры, depth buffer
- **C1-C2**: Математические типы на glam, базовые ошибки
- **D1**: Transform и Camera компоненты с MVP матрицами
- **D2**: Простая ECS система для управления объектами сцены

### В разработке 🚧

- Загрузка мешей и текстур (крейт `asset`)
- Система материалов и освещение
- Оптимизация рендеринга (батчинг, сортировка)
- FrameGraph для управления проходами рендеринга

### Планируется 📋

- PBR материалы (metallic/roughness)
- Система теней (shadow mapping)
- Culling и LOD системы
- Профилирование GPU
- Тестирование "золотых кадров"

Подробный план развития доступен в [TODO.md](TODO.md).

## 🛠️ Разработка

### Структура кода

- **Комментарии**: На русском языке для ясности
- **Логирование**: На английском языке
- **Тестирование**: Unit тесты для математики и core компонентов
- **Ошибки**: Использование `anyhow` и `thiserror` для обработки ошибок

### Настройка логирования

```bash
# Включить debug логи
RUST_LOG=debug cargo run -p app

# Логи только от renderer
RUST_LOG=renderer=debug cargo run -p app

# Подробные логи wgpu
RUST_LOG=wgpu=info,svarog3d=debug cargo run -p app
```

### Тестирование

```bash
# Запуск всех тестов
cargo test --workspace

# Тесты конкретного крейта
cargo test -p corelib
```

## 🎮 Управление

В текущей версии реализована демонстрация вращающегося куба:

- Автоматическое вращение по времени
- Поддержка изменения размера окна
- Корректная обработка depth buffer

## 🤝 Участие в разработке

Проект находится в активной разработке. Приветствуются:

- Отчеты об ошибках
- Предложения по улучшению архитектуры
- Реализация новых возможностей согласно roadmap
- Оптимизации производительности

### Кодстайл

- Следование стандартам Rust 2024
- Документирование публичного API
- Покрытие тестами критичных компонентов
- Осознанное управление зависимостями

## 📄 Лицензия

Проект распространяется под лицензией MIT. См. [LICENSE](LICENSE) для подробностей.

## 🔗 Технологии

- **[wgpu](https://wgpu.rs/)** - Кроссплатформенная графическая библиотека
- **[winit](https://github.com/rust-windowing/winit)** - Оконная система
- **[glam](https://github.com/bitshifter/glam-rs)** - Математическая библиотека для графики
- **[bytemuck](https://github.com/Lokathor/bytemuck)** - Безопасные приведения типов
- **[log](https://github.com/rust-lang/log)** + **[env_logger](https://github.com/env-logger-rs/env_logger/)** - Логирование

---

**Svarog3D** - создан с ❤️ на Rust для изучения современной 3D графики и архитектуры игровых движков.
