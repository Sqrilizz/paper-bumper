# paper-bumper

Инструмент и GitHub Action на Rust для автоматического обновления версий плагинов Minecraft (PaperMC / Purpur) и верификации через запуск headless сервера (Smoke Test).

Автоматически отслеживает новые релизы Minecraft/Paper, обновляет конфигурационные файлы сборки (`gradle.properties`, `build.gradle.kts`, `pom.xml`, `paper-plugin.yml`), компилирует проект и разворачивает изолированный тестовый сервер в CI для проверки реального запуска и работы плагина.

---

## Возможности

- **Интеграция с PaperMC и Purpur API**: Получение свежих сборок через `fill.papermc.io` и `api.purpurmc.org`.
- **Поддержка Gradle и Maven**: Корректно находит и обновляет версии в `gradle.properties`, `build.gradle.kts`, `build.gradle`, `pom.xml`.
- **Обновление дескрипторов**: Автоматически приводит `api-version` в `paper-plugin.yml` и `plugin.yml` к актуальной мажорной версии.
- **Headless Server Smoke Test**:
  - Автоматически скачивает официальный JAR ядра нужной версии.
  - Генерирует конфиг сервера с оптимизированными параметрами запуска (`eula=true`, плоский мир, минимальный тик-тайм, без GUI).
  - Копирует собранный JAR плагина в папку `plugins/`.
  - Мониторит поток логов сервера в реальном времени, проверяет строчку `onEnable()` и отсутствие исключений/стектрейсов.
  - Посылает команды в консоль сервера (`version`, `plugins` или кастомные команды плагина).
  - Корректно завершает сервер (`stop`) и проверяет `onDisable()`.
- **Автоматический Pull Request**: Создает отдельную ветку и открывает PR с подробным markdown-отчетом и логами запуска.
- **Поддержка аватарки и бейджа бота**: Полная поддержка GitHub App токенов для коммитов от лица официального `[bot]` с вашей аватаркой.

---

## Подключение в GitHub Actions

Создайте в репозитории плагина файл `.github/workflows/paper-update.yml`:

```yaml
name: Auto Update Minecraft Plugin

on:
  schedule:
    - cron: '0 0 * * *'
  workflow_dispatch:

jobs:
  update:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write

    steps:
      - name: Generate Bot Token (Optional)
        id: app_token
        if: ${{ env.APP_ID != '' && env.APP_PRIVATE_KEY != '' }}
        uses: actions/create-github-app-token@v1
        with:
          app-id: ${{ secrets.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}
        env:
          APP_ID: ${{ secrets.APP_ID }}
          APP_PRIVATE_KEY: ${{ secrets.APP_PRIVATE_KEY }}

      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          token: ${{ steps.app_token.outputs.token || github.token }}

      - name: Set up Java 21
        uses: actions/setup-java@v4
        with:
          distribution: 'temurin'
          java-version: '21'

      - name: Run Paper Bumper
        uses: Sqrilizz/paper-bumper@main
        with:
          software: 'paper'
          channel: 'STABLE'
          auto-pr: 'true'
          smoke-test: 'true'
          test-commands: 'version,plugins'
          github-token: ${{ steps.app_token.outputs.token || github.token }}
          author-name: ${{ steps.app_token.outputs.app-slug && format('{0}[bot]', steps.app_token.outputs.app-slug) || 'github-actions[bot]' }}
          author-email: ${{ steps.app_token.outputs.app-slug && format('{0}[bot]@users.noreply.github.com', steps.app_token.outputs.app-slug) || 'github-actions[bot]@users.noreply.github.com' }}
```

---

## Параметры Action

| Параметр | Описание | По умолчанию |
|---|---|---|
| `project-dir` | Путь к корню плагина | `.` |
| `software` | Ядро сервера (`paper` или `purpur`) | `paper` |
| `channel` | Канал релизов (`STABLE` или `EXPERIMENTAL`) | `STABLE` |
| `target-version` | Принудительная версия Minecraft (например, `26.3`) | *(последняя из API)* |
| `smoke-test` | Запуск проверки на реальном headless сервере | `true` |
| `test-commands` | Список команд через запятую для отправки на сервер | `version,plugins` |
| `timeout` | Таймаут ожидания старта сервера в секундах | `120` |
| `auto-pr` | Создавать Pull Request при успешных тестах | `true` |
| `github-token` | GitHub токен для пуша и создания PR | `${{ github.token }}` |
| `author-name` | Имя автора git-коммита | `github-actions[bot]` |
| `author-email` | Email автора git-коммита | `github-actions[bot]@users.noreply.github.com` |

---

## Локальный запуск CLI

Сборка бинарника:
```bash
cargo build --release
```

Проверка наличия новой версии:
```bash
./target/release/paper-bumper check --software paper
```

Обновление файлов версий в проекте:
```bash
./target/release/paper-bumper bump --software paper
```

Изолированный запуск Smoke Test для уже собранного плагина:
```bash
./target/release/paper-bumper smoke-test \
  --server-jar ./paper.jar \
  --plugin-jar ./build/libs/MyPlugin.jar \
  --commands "version,plugins"
```

Полный автоматический прогон (проверка -> бамп -> сборка -> Smoke Test):
```bash
./target/release/paper-bumper run --software paper
```
