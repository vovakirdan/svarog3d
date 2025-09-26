# План реализации (блоки → подблоки с DoD)

## Блок A — Платформа

**A1. Окно и событийный цикл (winit)**
**Описание:** создать окно, обрабатывать `CloseRequested`, `Resized`, `ScaleFactorChanged`.
**Ожидаемый результат:** приложение открывает окно 1280×720, корректно реагирует на ресайз, без падений, CPU < 5% в простое.

**A2. Логирование и CLI-флаги**
**Описание:** подключить `env_logger`, флаги `--gpu-backend=auto|vulkan|dx12|metal` (парсинг через простейший `std::env`).
**Ожидаемый результат:** по переменной `RUST_LOG` видим отладочные/инфо логи; переданный backend отражается в логах и применяется, при невалидном — fallback с предупреждением.

---

## Блок B — GPU backend (wgpu)

**B1. Инициализация wgpu + surface**
**Описание:** создать `Instance/Adapter/Device/Queue`, выбрать sRGB формат, сконфигурировать поверхность, очищать экран цветом.
**Ожидаемый результат:** стабильный «clear screen», без ошибок Surface; при сворачивании/разворачивании — восстанавливается.

**B2. Простой пайплайн + WGSL**
**Описание:** вершинный буфер треугольника, минимальные `vs/fs`, пайплайн и отрисовка.
**Ожидаемый результат:** цветной треугольник; при `Lost/Outdated` — корректная реконфигурация поверхности, без крашей.

**B3. Depth buffer**
**Описание:** добавить depth-текстуру, включить тест глубины, корректный recreate на ресайз.
**Ожидаемый результат:** вращающийся куб не «просвечивает» себя; после ресайза depth работает.

---

## Блок C — Core/Math (минимум зависимостей)

**C1. Матрицы/векторы (`glam`)**
**Описание:** `Vec3/Mat4`, `perspective`, `look_at`, базовые утилиты, unit-тесты.
**Ожидаемый результат:** детерминированные тесты для математики (несколько assert по известным матрицам/векторам), без доп. зависимостей.

**C2. Общие типы/ошибки**
**Описание:** `CoreError/CoreResult`, id/handle ресурсов, базовые DTO.
**Ожидаемый результат:** единая ошибка сквозь слои, компиляется без circular deps.

---

## Блок D — Сцена и камера

**D1. Transform & Camera**
**Описание:** компоненты `Transform{pos,rot,scale}`, `Camera{proj,view}`, UBO для MVP.
**Ожидаемый результат:** вращающийся **куб** (позиции/индексы), камера управляет видом (хотя бы орбита по времени).

**D2. Мини-ECS (без зависимостей)**
**Описание:** простой реестр сущностей (вектора компонентов); рендер списка мешей с разными Transform.
**Ожидаемый результат:** сцена из N кубов (N настраиваемый), FPS стабилен, без GC/alloc в кадре.

---

## Блок E — Ресурсы и парсеры

**E1. Меши**
**Описание:** минимальный парсер OBJ (позиции/нормали/uv) или свой формат; загрузка в GPU-буферы.
**Ожидаемый результат:** отрисовка внешнего меша (например, Suzanne), корректный winding/culling.

**E2. Текстуры**
**Описание:** загрузка RGBA8, создание сэмплера, (опционально) мипы.
**Ожидаемый результат:** отображение диффузной текстуры на меше, билинейная фильтрация.

---

## Блок F — Материалы и шейдеры

**F1. Bind groups/Uniforms**
**Описание:** разнести буферы: камера/модель/материал; унифицированный layout.
**Ожидаемый результат:** можно переключать материал и шейдер без переделки пайплайна; чёткие `bind_group` слоты.

**F2. Освещение (Lambert/Blinn-Phong)**
**Описание:** базовый диффуз/спекуляр, 1–2 источника света, нормали из вершин/тангент с нормалмап (опционально).
**Ожидаемый результат:** визуально корректные блики и затенения; параметры света задаются и влияют на кадр.

---

## Блок G — Производительность и пайплайн-проходы

**G1. Подготовка команд и сортировка**
**Описание:** батчинг/сортировка по PSO/материалу/текстуре, минимизация state changes.
**Ожидаемый результат:** на сцене с десятками мешей «дрожь» FPS исчезает; лог показывает уменьшение переключений пайплайнов.

**G2. Мини-FrameGraph**
**Описание:** явные пассы (gbuffer → lighting → post), зависимости ресурсов, удобный API.
**Ожидаемый результат:** добавить постэффект (гамма/тонмап) — не трогая остальной код.

---

## Блок H — Расширения (по желанию)

**H1. Тени (shadow map)** — depth-pass с точки зрения света, сэмплинг в основном пассе.
**H2. PBR (metallic/roughness, IBL)** — материалы уровня «современная графика».
**H3. Culling/LOD** — frustum, затем occlusion (optional), уровни детализации.
**H4. Профайлинг** — GPU timestamps, overlay с миллисекундами.
**H5. Тесты «золотых кадров»** — сравнение картинок для регрессий.

---

## Блок I — Пользовательский интерфейс

**I1. Интеграция egui**
**Описание:** встроить immediate-mode GUI для инспекторов, настроек, иерархии сцены.
**Ожидаемый результат:** overlay панели поверх 3D вида; регулировка параметров света/камеры в реальном времени, FPS counter в углу.

**I2. Viewport и управление камерой**
**Описание:** орбитальная камера (мышь + клавиши), pan/zoom, gizmo для трансформаций объектов.
**Ожидаемый результат:** intuitive 3D navigation как в Blender/Unity; выделение объектов кликом, перемещение/поворот через визуальные handles.

**I3. Иерархия сцены и инспектор**
**Описание:** дерево объектов с возможностью выделения, переименования; панель свойств для Transform/Material.
**Ожидаемый результат:** полноценный scene outliner; изменение параметров через UI мгновенно отражается в 3D виде.

---

## Блок J — Форматы и импорт

**J1. Расширенный парсер моделей**
**Описание:** поддержка FBX/glTF 2.0, анимации, multiple materials per mesh, иерархии костей.
**Ожидаемый результат:** импорт сложных моделей из DCC tools (Blender/Maya); корректное отображение multiple materials и UV-каналов.

**J2. Система материалов**
**Описание:** node-based материалы или JSON-дескрипторы, библиотека preset'ов, hot-reload шейдеров.
**Ожидаемый результат:** drag&drop текстур на материалы, live editing параметров без перекомпиляции приложения.

**J3. Проектные файлы**
**Описание:** сериализация/десериализация сцены (RON/JSON), система asset dependencies.
**Ожидаемый результат:** сохранение/загрузка проектов, автоматический reimport при изменении source assets.

---

## Блок K — Рендеринг высокого качества

**K1. Physically Based Rendering (PBR)**
**Описание:** metallic/roughness workflow, IBL с environment maps, BRDF lookup tables.
**Ожидаемый результат:** photorealistic материалы, HDR skybox, корректные отражения и subsurface scattering basics.

**K2. Advanced Lighting**
**Описание:** punctual lights (spot/point/directional), area lights, light probes, cascaded shadow maps.
**Ожидаемый результат:** динамическое освещение multiple источников, soft shadows, indirect illumination approximation.

**K3. Post-processing Pipeline**
**Описание:** tone mapping, bloom, SSAO, temporal anti-aliasing (TAA), color grading.
**Ожидаемый результат:** cinematic image quality, настраиваемые post-effect chains через FrameGraph.

---

## Блок L — Производительность и оптимизация

**L1. Advanced Culling**
**Описание:** frustum culling, occlusion culling с GPU queries, level-of-detail (LOD) system.
**Ожидаемый результат:** stable 60+ FPS на сценах с тысячами объектов, automatic LOD transitions.

**L2. GPU-Driven Rendering**
**Описание:** indirect drawing, GPU culling через compute shaders, mesh shaders (если поддерживается).
**Ожидаемый результат:** минимальный CPU overhead, масштабирование на large worlds без CPU bottleneck.

**L3. Memory Management**
**Описание:** streaming system для больших сцен, texture compression, geometry compression.
**Ожидаемый результат:** поддержка multi-GB сцен при ограниченной VRAM, seamless loading/unloading.

---

## Блок M — Инструменты разработки

**M1. Asset Pipeline**
**Описание:** автоматическая конвертация FBX→optimized format, texture compression, dependency tracking.
**Ожидаемый результат:** build system который обрабатывает изменения в source assets, optimal runtime formats.

**M2. Debugging Tools**
**Описание:** wireframe mode, normal visualization, light debugging, GPU profiler integration.
**Ожидаемый результат:** visual debugging overlay, frame timing breakdowns, bottleneck identification.

**M3. Scripting Integration**
**Описание:** Lua/WASM scripting для game logic, hot-reload, visual scripting nodes (опционально).
**Ожидаемый результат:** rapid prototyping без перекомпиляции, accessible для non-programmers.

---

## Блок N — Экосистема и распространение

**N1. Plugin Architecture**
**Описание:** динамическая загрузка модулей, stable API для external renderers/importers.
**Ожидаемый результат:** third-party plugins расширяют функциональность, backward compatibility.

**N2. Export и Integration**
**Описание:** экспорт сцен в popular formats, headless rendering для automation, CLI interface.
**Ожидаемый результат:** integration в production pipelines, batch processing, render farms compatibility.

**N3. Documentation и Community**
**Описание:** comprehensive docs, tutorials, example projects, contributor guidelines.
**Ожидаемый результат:** accessible для новых пользователей, sustainable open-source project.

---

## Контроль прогресса — вехи (быстрый чек-лист)

### Фундаментальные возможности (Completed ✅)
* **M0:** Скелет репо собирается (`cargo run -p app`). ✅
* **M1:** Окно живёт/ресайзится (A1). ✅
* **M2:** Clear color + треугольник (B1–B2). ✅
* **M3:** Вращающийся куб с depth (B3, D1). ✅
* **M4:** Suzanne с текстурой (E1–E2). ✅
* **M5:** Свет/материалы (F2). ✅
* **M6:** Сцена из N мешей, стаб. FPS (D2, G1). ✅
* **M7:** Постпроц через мини-FrameGraph (G2). ✅

### Расширенная графика
* **M8:** Тени/PBR/профайлинг (H*).
* **M9:** Орбитальная камера + basic UI (I1-I2).
* **M10:** Иерархия сцены + инспектор (I3).

### Профессиональные инструменты
* **M11:** glTF импорт + multi-material (J1-J2).
* **M12:** Проектные файлы + asset pipeline (J3, M1).
* **M13:** Physically-based рендеринг (K1-K2).
* **M14:** Post-processing chain (K3).

### Производительность
* **M15:** Advanced culling + LOD system (L1).
* **M16:** GPU-driven rendering (L2).
* **M17:** Streaming + memory management (L3).

### Экосистема
* **M18:** Debugging tools + profiler (M2).
* **M19:** Scripting integration (M3).
* **M20:** Plugin architecture (N1).
* **M21:** Export pipeline + CLI (N2).
* **M22:** Production-ready documentation (N3).
