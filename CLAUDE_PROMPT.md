# Промпт для Claude · Winteria Android

Клод, продолжай разработку Winteria с актуальной официальной версии. Рабочий репозиторий — приватный `dozyFJ/Vinteria`, ветка `winteria`. У GitHub-аккаунта `ffight763-glitch` подтверждено право записи в этот репозиторий. Используй его для доступа, если твоя текущая сессия не видит ветку. [Открыть ветку](https://github.com/dozyFJ/Vinteria/tree/winteria)

## С чего начать

1. Открой актуальную `winteria` и прочитай `AGENTS.md` и конец `CLAUDE_HANDOFF.md`. Сверь последнюю опубликованную версию с OTA. На момент передачи это **1.0.72**, исходники сборки — `b351e59`. Упоминание о выпуске 1.0.63 в начале `AGENTS.md` историческое. [Манифест OTA](https://raw.githubusercontent.com/dozyFJ/pulse-updates/winteria/latest.json)
2. Если продолжаешь работу над мобильной версией, проверь затронутые сценарии. В Playlist горизонтальный свайп по имени открывает действия, вертикальный жест группирует дорожки, а Select и Clone работают с Audio и MIDI. Undo и Redo должны восстанавливать результат. Эти сценарии прогнаны в мобильном Chromium. Отмечай отдельно то, что проверил на Android.
3. Запиши изменения и результаты проверки в `CLAUDE_HANDOFF.md`. Пушь исходники в общую ветку `winteria` обычным push. Публичное зеркало `ffight763-glitch/Vinteria-git3` предназначено для сборки через GitHub Actions. Перед сборкой в нём сверь коммит с приватной веткой.

## Сборка и подпись

Официальный Android-пакет — `com.winteria.daw`. Для установки поверх опубликованной версии нужен прежний сертификат SHA-256:

```text
f8dd8eaed875b0aa5896e1d23cbec407fd7b4e38da634e866ca139a408d18523
```

Для официальной сборки workflow использует GitHub Actions Secrets `ANDROID_KEYSTORE_B64` и `ANDROID_KEYSTORE_PASSWORD`. Ключ находится в среде сборки Codex. Текущая интеграция GitHub получает HTTP 403 на доступе к Secrets. Если секреты пока недоступны в твоей среде, передай Codex SHA готового коммита и результаты проверок. Codex соберёт APK прежним ключом. Если нужна тестовая сборка, используй существующий вариант Winteria Dev с пакетом `com.winteria.daw.dev`.

При согласованном выпуске сначала запушь исходники, затем опубликуй проверенный APK и `latest.json` в `dozyFJ/pulse-updates:winteria`. Проверь скачивание APK, сертификат, SHA-256 и соответствие `sourceCommit` коммиту сборки. Ключ и пароль используй только через секреты CI или защищённую среду сборки.

## Команды для следующей сборки

Запускай в корне рабочей ветки. `CARGO_APK_RELEASE_KEYSTORE` содержит путь к keystore, а `CARGO_APK_RELEASE_KEYSTORE_PASSWORD` передаётся через окружение CI или хранилище секретов. Значения секретов в Markdown не хранятся.

```bash
git fetch origin winteria
git switch winteria
git pull --ff-only origin winteria
./tools/build-android.sh --release
apksigner verify --print-certs /path/to/Winteria.apk
sha256sum /path/to/Winteria.apk
```

Путь к APK выводит скрипт сборки. Для OTA сверяй сертификат с отпечатком в этом файле и APK со значением `sha256` в опубликованном манифесте.
