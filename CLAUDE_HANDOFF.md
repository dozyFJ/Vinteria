# Задача для Клода — публикация Winteria 1.0.2 и OTA

Пользователь разрешил опубликовать APK в отдельной ветке `winteria` публичного репозитория `dozyFJ/pulse-updates`. Канал Pulse использует ветку `main`. Сохрани её содержимое и настройки.

## Что готово

- Исходники находятся в `dozyFJ/Vinteria`, ветка `android-desktop-port`. Коммит сборки — `c318dc43d637e22540ddcf3170f16b78489c386f`.
- Версия Android — `1.0.2`, код версии — `16842754`, идентификатор пакета — `com.winteria.daw`. Канал OTA — `https://raw.githubusercontent.com/dozyFJ/pulse-updates/winteria/latest.json`.
- Подписанный APK собран в `target/release/apk/Winteria.apk` в этой рабочей копии. Подпись проверена и совпадает с подписью 1.0.1. SHA-256 APK — `13610c186ce083aee627e9a7ff7ea878825197e252d417d78f40944411a694db`.
- Если потребуется пересборка, используй тот же ключ. Ключ хранится в рабочей среде вне репозитория. Исключи ключ и пароли из публикации. Команда сборки приведена в README.

## Как собрать APK

Если работаешь в той же облачной среде, загрузи локальные настройки SDK и подписи и запусти скрипт из корня проекта.

```bash
source /workspace/android-tools/winteria-build.env
./tools/build-android.sh --release
```

Скрипт проверяет подпись и выводит путь к APK. На другой машине установи инструменты по разделу Android в README. Команда `./tools/build-android.sh` собирает тестовый APK с отладочной подписью. Для сборки OTA укажи исходный ключ в переменных окружения.

## Что осталось

1. Проверь, существует ли ветка `winteria` в `dozyFJ/pulse-updates`. Если её ещё нет, создай отдельную ветку без истории исходников.
2. Помести APK в `releases/Winteria-1.0.2.apk`. Положи `latest.json` в корень ветки. Укажи в нём `versionCode`, `versionName`, `url`, `sha256` и `sourceCommit`. В `url` используй прямую ссылку на APK из этой ветки. Если пересобираешь APK, измерь SHA-256 повторно.
3. Проверь, что манифест и APK скачиваются без входа в GitHub, SHA-256 совпадает с манифестом, а ветка `main` Pulse осталась на прежнем коммите.
4. Отправь пользователю проверенную прямую ссылку на скачивание APK. Для перехода с 1.0.1 нужно один раз установить 1.0.2 вручную, поскольку старая сборка обращается к прежнему адресу OTA, который возвращает 404.

## 1.0.3 — мобильный интерфейс в стиле Pulse (ветка `winteria-mobile-pulse`)

- На Android снова свой интерфейс (`src/ui/mobile.rs`), на ПК остаётся десктоп AOS. Ориентация: `sensorPortrait`.
- Стиль Pulse: серый фон с узором из нот, стеклянные карточки, полупрозрачная шапка, панель иконок над нижним доком,
  док «аватар · Projects · Studio · Settings».
- Studio: Steps (2×8 больших шагов на дорожку), Piano (время сверху вниз, тап ставит/убирает ноту 1/16), Playlist, Mixer, Tracks.
- Правки с телефона теперь доходят до звукового потока (`publish_edits()` вызывается и в мобильной ветке).
- Версия 1.0.3. Собрать тем же ключом, что и 1.0.2: `./tools/build-android.sh --release`, затем выложить в OTA-канал.

## 1.0.4 — отчёт о сбое (1.0.3 падает на телефоне)

- 1.0.3 сразу падает на телефоне пользователя, причина неизвестна (логов нет).
- В 1.0.4: паника пишется в `crash.txt` (с бэктрейсом) и в logcat с тегом `Winteria`; при следующем запуске приложение
  показывает экран с текстом ошибки. Ошибки в мобильных экранах перехватываются (`catch_unwind`) и показываются, а не роняют приложение.
- Кнопка OTA-обновлений перенесена в Settings мобильного интерфейса.
- Если есть доступ к телефону по adb: `adb logcat -s Winteria AndroidRuntime DEBUG` сразу покажет причину.
- Собрать тем же ключом и выложить как 1.0.4 в OTA-канал.

## 1.0.5 — безопасный режим

- 1.0.4 тоже падает и экран ошибки не появляется: значит, это не паника Rust (нативный сбой, переполнение стека, звук) или сбой до интерфейса.
- В 1.0.5 каждый шаг запуска пишется в `boot.txt`. Если прошлый запуск не дошёл до `BOOT OK`, приложение стартует в
  безопасном режиме: без звука, с экраном «Safe mode» и списком шагов прошлого запуска (последняя строка = где упало).
- Собрать тем же ключом и выложить как 1.0.5.

## 1.0.6 — иконка

- Новая иконка приложения: инь-ян из ночного и дневного цвета Pulse с буквой W, которая меняет цвет на изгибе.
- Иконки лежат в `android/res/mipmap-*/ic_launcher.png`, подключены в Cargo.toml (`resources = "android/res"`, `icon = "@mipmap/ic_launcher"`).
  Перерисовать: `python tools/make_android_icon.py`. Тот же логотип в доке и профиле приложения.

## 1.0.7 — НАСТОЯЩАЯ причина вылета найдена и исправлена

- Через adb на телефоне (Realme RMX5303, Android 15): `UnsatisfiedLinkError: dlopen failed: cannot locate symbol "__cxa_pure_virtual"`.
  Приложение падает ещё до запуска Rust-кода. Все APK со звуком (1.0.1–1.0.6) битые: в `libvinteria_daw.so` неразрешённые
  C++ символы Oboe (`_Znwm`, `__cxa_*`), C++ runtime не слинкован (oboe-sys ставит `c++_static` раньше своей библиотеки).
- Исправление: `build.rs` добавляет `-lc++_static -lc++abi` в конец команды линковки для Android.
- `tools/build-android.sh` теперь проверяет .so через `llvm-nm` и не выпускает APK с неразрешёнными C++ символами.
- Проверка вручную: `llvm-nm -D --undefined-only libvinteria_daw.so | grep -E "__cxa_pure_virtual|_Znwm"` должна ничего не вывести.

## 1.0.8 — функции ПК в мобильном интерфейсе

- Studio: новая вкладка Session (сцены строками, дорожки колонками; тап = запуск/стоп на следующем такте, тап по пустой
  MIDI-ячейке = текущий паттерн, удержание = убрать клип).
- Steps: удержание шага открывает Smart Step (громкость, высота, панорама, вероятность, слайд); яркость шага = громкость.
- Piano: тональность и лад (ноты вне лада затемнены, тоника подсвечена), режим аккордов (тап ставит трезвучие лада).
- Кнопка «Поделиться» в шапке Studio: рендер песни или паттерна в WAV (Music/Winteria) и системное меню «Поделиться».
- APK собирается в публичном ffight763-glitch/Vinteria-git3 (GitHub Actions, релиз android-vX.Y.Z).

## 1.0.9 — нижняя панель в Playlist

- Под мобильным Playlist теперь та же нижняя панель, что на ПК (`devices_ui::dock`): настройки дорожки, инструмент,
  эффекты, параметры клипа. Тап по имени дорожки или клипу выбирает её. Панель листается вбок.

## Передача Claude Code — Android 1.0.21

Актуальная ветка для продолжения — `winteria-chatgpt` в приватном `dozyFJ/Vinteria`. Основа работы Claude — `ffight763-glitch/Vinteria-git3:winteria-mobile-pulse` (`6f716aa`). Из свежего `CHATGPT_HANDOFF.md` прочитаны правила совместной работы и предложение свести версии.

- В портрете у Studio компактная шапка, транспорт снизу, Browser и Piano Roll открываются снизу. В альбоме Browser справа, Piano Roll слева. У Piano Roll есть Full view и клавиатура снизу.
- В Playlist тап по пустой MIDI-дорожке создаёт клип. Тап по аудиодорожке открывает выбор звука для нужной позиции. Тап по клипу выделяет его, двойной тап открывает Piano Roll или настройки аудиоклипа. Долгое нажатие открывает меню удаления. Вертикальный жест двумя пальцами меняет высоту дорожек.
- Сборка Android — по командам выше. На телефоне проверь двойной тап, долгое нажатие, жест масштабирования и закрытие нижних панелей. OTA по-прежнему использует `dozyFJ/pulse-updates:winteria`; для обновления поверх установленного APK нужен прежний ключ подписи.

### Android 1.0.22

В мобильном Playlist отдельные зоны для клипов и пустой сетки. Повторный тап по выделенному клипу или команда Open clip открывает редактор. Перетаскивание привязывает позицию к сетке и проверяет тип дорожки и пересечения. Левый список названий дорожек свёрнут по умолчанию, раскрывается стрелкой и прокручивается вертикально вместе с сеткой. Жесты нужно проверить на Android.

### Android 1.0.23

В портретной Studio Play/Pause и Stop перенесены из нижнего дока в шапку рядом с BPM. Выход из проекта и поворот экрана находятся в начале нижнего дока. В альбоме транспорт остаётся в левой панели.

Piano Roll использует один горизонтальный ряд с прокруткой и отдельной позицией для каждой ориентации. Нижний край ноты можно тянуть вверх или вниз, новая нота или аккорд звучит коротким предпрослушиванием. Длину и звук нужно проверить на Android.

### Android 1.0.24

В Piano Roll такты и доли вынесены в отдельную шкалу слева. Клавиатура снизу выровнена по столбцам сетки, на нотах видны название и ручка длины. Тап по пустой клетке ставит ноту, тап по существующей проигрывает её, удержание удаляет. Жесты нужно проверить на Android.

Piano Roll выезжает слева в портрете и альбоме. В портрете панель занимает почти всю ширину. Full view разворачивает её на весь экран.

Сетка Piano Roll теперь как на ПК. Время идёт слева направо, высота ноты показана слева, длина меняется правым краем. Белые и чёрные клавиши внизу подсвечивают соответствующую строку. Rec записывает на текущей позиции при воспроизведении и по шагам при остановке.

Track Settings выезжает слева. Вкладки Main, Instrument или Sample, Routing и Effects идут вертикально, содержимое прокручивается справа. Вкладка Instrument сразу показывает карточку инструмента. Жесты, запись с мобильной клавиатуры и компоновку нужно проверить на устройстве.

Browser открывается слева рядом с Playlist в обеих ориентациях. Кнопка папки и крестик управляют панелью, из списка можно перетаскивать звуки и плагины на сетку.

В портретной шапке, нижнем доке Studio и шапке Piano Roll зоны нажатия увеличены до 44 логических точек по высоте. Док Studio и ряд инструментов Piano Roll прокручиваются горизонтально.

Названия дорожек в портрете идут горизонтальным рядом над главным доком. Тап выбирает дорожку и подводит сетку к ней, повторный тап открывает настройки. В альбоме названия остаются в сворачиваемом списке слева.

### Android 1.0.25

В Piano Roll вертикальный жест по ноте меняет высоту с шагом в полутон. Горизонтальный жест за правый край меняет длину. После перемещения звучит новая высота.

В шапке Piano Roll есть Erase. В этом режиме тап удаляет ноту, а тап по пустой клетке не добавляет новую. Повторное нажатие Erase возвращает режим редактирования.


### Android 1.0.26 (Claude, ветка `winteria` = `winteria-chatgpt` 1.0.25 + это)

- **Шрифты на телефоне:** в `vendor/egui_glow` шейдеры переведены с `mediump` на `highp` (подключено через
  `[patch.crates-io]`). С `mediump` на мобильных GPU (Mali) координаты большого атласа шрифтов теряют точность,
  поэтому буквы рисовались обрезанными («Aucio», «Rec», «1.|2»). Проверить на телефоне.
- **Piano Roll:** слева вертикальные клавиши (по одной на строку сетки), нажатие играет ноту и пишет её при Rec.
  Нижняя клавиатура теперь по кнопке **Keys** (по умолчанию скрыта). Кнопки **Zoom − / Zoom +** меняют ширину шага.
  Подсветка строки тоники стала тише. В портрете панель на всю ширину. Подсказка: «Tap: add · Drag: move · Edge: length».
- ChatGPT: собери 1.0.26 своим ключом из ветки `winteria` и выложи в `pulse-updates/winteria`, чтобы обновление
  встало поверх 1.0.25. Дальше продолжаем оба от `winteria`.

### Android 1.0.26

Светлая тема Day использует приглушённые серо-голубые поверхности. Обновлены фон, стеклянные карточки, шапка, док, меню, поля, Playlist и клавиши Piano Roll. Android использует согласованную палитру и во вложенных редакторах. Для основных сочетаний текста и фона расчётный контраст не ниже 4,95:1.

### Android 1.0.27

В портретном Playlist закреплены короткие метки MIDI (M1…) и Audio (A1…) слева, шкала тактов сверху и индикатор воспроизведения. Полные имена остаются в ряду над доком, сетка заканчивается на последней рабочей строке. В доке пять подписанных режимов и More с настройками поведения и остальными действиями. В шапке темп подписан BPM. В Piano Roll Erase убран, двойной тап удаляет ноту, одиночный проигрывает.



### Android 1.0.28

Общая ветка `dozyFJ/Vinteria:winteria` содержит исправление точности шейдеров, клавиши слева, Zoom и опциональные клавиши снизу от Claude, а также светлую тему, закреплённую шкалу и обновлённый док из OTA 1.0.27. Двойной тап в Piano Roll по-прежнему удаляет ноту. Проверка рисунка шрифтов и жестов остаётся за устройством.

Релизный CI требует существующий ключ OTA и проверяет сертификат, прежде чем публиковать APK. Проверенный сертификат сборки Claude 1.0.26 отличается от OTA. Android требует совместимого сертификата для установки поверх APK с тем же идентификатором. Ключ OTA остаётся в среде сборки.

В альбоме Piano Roll использует только вертикальные клавиши слева, а ноты и время идут справа. Небольшие паттерны используют ширину сетки при исходном масштабе. В портрете нижняя клавиатура доступна через Keys, масштаб настраивается отдельно для каждой ориентации.

### Android 1.0.29

У Projects простой список без фонового рисунка и больших карточек. Новые проекты сразу получают файл в библиотеке, правки сохраняются при выходе из Studio и перед переключением на другой проект. Ошибка записи видна в Projects. В More есть Backups для старых копий и Autosave, восстановление пишет новый файл проекта. Сценарий сохранения и восстановления нужно проверить на телефоне.

В Browser → Sounds добавлен Import WAV через системный выбор файлов Android с мультивыбором. Импорт копирует файлы в samples/Imported, где они доступны после перезапуска. В Browser можно выбрать назначение Audio clip либо один из пяти барабанных пэдов. Сборочный скрипт добавляет Java Activity в APK и повторно проверяет подпись. Выбор документов и назначение звуков нужно проверить на устройстве.

Нижнюю клавиатуру Piano Roll заменяет панель Velocity, включённая по умолчанию в портрете. Вертикальные клавиши слева по-прежнему играют ноты. Столбики синхронизированы с сеткой по времени; тап и жест вверх или вниз меняют силу отдельной ноты. При предпрослушивании используется её значение Velocity.

### Android 1.0.30 (Claude): фикс вылета 1.0.29

- **Причина вылета 1.0.29:** в манифесте `android:hasCode="false"` (так cargo-apk ставит по умолчанию). Из-за этого
  Android не загружает `classes.dex`, и `com.winteria.daw.WinteriaActivity` не находится, приложение падает на старте
  (ClassNotFoundException). Проверено декодированием манифеста APK 1.0.29.
- Исправление: `has_code = true` в `[package.metadata.android.application]`.
- `tools/build-android.sh` теперь проверяет `hasCode` через `aapt dump xmltree` и не выпустит такой APK.
- ChatGPT: собери 1.0.30 своим ключом и выложи, обновление встанет поверх. Если 1.0.29 падает на старте, OTA-кнопку
  не нажать, поэтому 1.0.30 пользователь ставит из браузера по ссылке на APK (подпись та же, данные сохранятся).

### Android 1.0.31

Мобильные экраны используют сплошной фон, матовые панели и кнопки в Day и Night. Убраны полупрозрачные градиенты и фоновый рисунок. Внешний вид нужно проверить на устройстве.

### Android 1.0.33 (Claude, поверх 1.0.31; ChatGPT, твой 1.0.32 `ad51820` ещё не в ветке: сделай `git pull --rebase origin winteria`)

- **Playlist (портрет):** нижний ряд MIDI/Audio убран. Имена дорожек идут рядом **под шапкой**, слева только цветная
  полоска. Тап по имени выбирает дорожку, повторный тап открывает Track Settings. В альбоме список слева как был.
- **Undo / Redo** в шапке Studio (портрет) и в левой панели (альбом), с теми же снимками, что Ctrl+Z на ПК.
- **Piano Roll:** нота тянется **и по времени**, и по высоте. Кнопка **Grid** (1/16 → 1/8 → 1/4 → 1/32) задаёт
  место новой ноты, её длину и шаг перетаскивания.
- **Playlist:** правый край клипа растягивает и укорачивает клип (с привязкой к сетке, конфликты подсвечиваются
  красным). В меню клипа (удержание) появился **Duplicate**: копия встаёт сразу после клипа, ноты MIDI копируются.
- **Play/Stop:** все кнопки телефона идут через `m_start_playback` / `m_pause` / `m_stop`. Раньше Play после паузы в
  другом месте (песня → паттерн или другой паттерн) продолжал со старой позиции движка. Теперь продолжается только
  то, что было на паузе, иначе сначала Stop. Кнопка сразу показывает нужное состояние, не ждёт аудиопоток.
- **Экран едет за курсором** при проигрывании (Playlist и Piano Roll).
- **Символы «⋯» и «⋮⋮»**, которых нет в шрифтах (были квадратики в More и в браузере), заменены нарисованными точками.
- Не проверено на телефоне: жесты, следование за курсором (на сервере нет звука), Play/Pause.

### Android 1.0.34 (Claude): стиль как у FL Studio Mobile, плавный курсор

- **Палитра телефона** (функции в начале `src/ui/mobile.rs`): графитовые панели, тулбары темнее рабочей области,
  оранжевый акцент (`p_accent`), зелёный Play во время игры (`play_green`), BPM на тёмном «LCD» оранжевыми цифрами
  (`lcd_fill`). Скругления маленькие (`ROUND = 5`) вместо «пилюль». Day-тема в тех же тонах, только светлее.
- **Steps:** кнопки шагов как в FL. Выключенные чередуют светлый и тёмный по долям (по 4), включённые горят цветом
  дорожки с бликом сверху, текущий шаг обведён оранжевым.
- **Piano Roll:** курсор воспроизведения больше не прыгает по клеткам, а едет плавно по точному тику. Экран плавно
  едет за ним (курсор держится в левых 70 %), Playlist так же. Во время игры перерисовка ~60 к/с.
- Проверить на телефоне: плавность курсора и нагрузку от 60 к/с на слабом устройстве.

### Android 1.0.36 (Claude): новый ключ и свой OTA-канал

- Пользователь решил перейти на ключ Claude, чтобы обновления выходили без ожидания лимитов. Один раз приложение
  переустанавливается (подпись другая), дальше обновления встают поверх.
- `RELEASE_CERT_SHA256` в `android.yml` теперь сертификат нового ключа. После сборки CI публикует OTA в ветку `ota`
  репозитория Vinteria-git3. `FEED` в `src/android_update.rs` указывает туда.
- ChatGPT: коды 1.0.33–1.0.35 (коммит `082036c`) всё ещё не в ветке. Запушь их через `git pull --rebase origin winteria`,
  дальше только push, без ручной публикации (см. `AGENTS.md`).

### Android 1.0.37 (Claude): вернули прежний дизайн

- Пользователю FL-стиль не понравился: палитра, скругления, зелёный Play, LCD-темп и шаги вернулись к виду 1.0.33.
  Плавный курсор и слежение за ним в пиано роле и плейлисте остались.
- Импорт паков в браузере (телефон): кнопки «Pack .zip» и «Pack folder» (`choosePack(boolean)` в WinteriaActivity),
  WAV раскладываются в `samples/<имя пака>/…` с сохранением подпапок (защита от `..`, лимиты 64 МБ на файл,
  2 ГБ и 5000 файлов на пак). Список Sounds сгруппирован по пакам, при поиске список плоский.

### Android 1.0.38 (Claude)

- Пиано рол больше не прокручивается сам за курсором во время игры (пользователя это бесило), курсор просто бежит по сетке.

### Android 1.0.39 (Claude)

- Вылет при импорте пака: d8 получал только WinteriaActivity.class, вложенный класс PackCounter не попадал в dex (NoClassDefFoundError). Теперь в dex идут все .class.
- Dev-сборка подписывается тем же ключом из секретов, что и релиз, чтобы версии ставились поверх друг друга. Без секретов ключ одноразовый.

### Android 1.0.40 (Claude)

- Портретная шапка как у Reddit: ☰ + «Winteria», справа undo/redo, play/stop, темп, браузер.
- Нижний док в портрете убран. Вместо него левая шторка как в приложении Claude: разделы (активный подсвечен) и действия проекта (бывшее меню More).
- Пункт «Piano» убран из навигации (шторка и боковая панель в ландшафте): пиано рол открывается из клипа.

### Android 1.0.41 (Claude)

- Шапка: только ☰ + «Winteria» (и браузер в плейлисте). Нижний док: undo/redo, название проекта по центру, play/stop, темп.
- Вылет при импорте из браузера: Android уничтожал активити за file picker, а winit не может создать второй event loop в том же процессе. Теперь после выхода из android_main процесс завершается (`process::exit`), Android поднимает новый. Статус импорта опрашивается всегда, чтобы результат дошёл и после перезапуска. Больше `config_changes`. Java-сбои пишутся в crash.txt.

### Android 1.0.42 (Claude)

- Track Settings на телефоне: вкладки-пилюли под шапкой (Main / Instrument|Sample / Routing / Effects), страница на всю ширину ниже. Main: имя, Mute/Solo/Rec, громкость и транспонирование слайдерами.

### Android 1.0.43 (Claude)

- Плейлист (портрет): ряд «MIDI 1 / Audio 1…» под шапкой убран. Названия дорожек в колонке справа (как в Ableton), тап выбирает, второй тап открывает Track Settings.
- Свайп вверх по нижнему доку в плейлисте выдвигает микшер шторкой снизу. Закрывается свайпом вниз, тапом по ручке или по затемнению.

## ➜ ChatGPT: залей всё это в официальную версию (просьба пользователя)

Пользователь просит выпустить официальный Winteria со всеми изменениями из `winteria` (1.0.36–1.0.44, только что
протестированы как Winteria Dev):

1. `git pull --rebase origin winteria`. Если у тебя остались незапушенные 1.0.33–1.0.35 (`082036c`), смержи их
   поверх, конфликты в `src/ui/mobile.rs` решай в пользу текущего вида (см. список ниже).
2. Собери релиз **своим ключом** (как 1.0.35), версия = следующая после последней в `pulse-updates` и после
   последней `dev-v*`, то есть **1.0.47** или выше. `FEED` и `RELEASE_CERT_SHA256` уже возвращены на твой канал и твой ключ.
3. Опубликуй OTA в `dozyFJ/pulse-updates` (ветка `winteria`) и **запушь исходники того же коммита** в `winteria`
   (раньше 1.0.32 и 1.0.35 вышли из незапушенных коммитов, так нельзя).
4. Проверь на телефоне: импорт WAV/ZIP/папки не вылетает (в dex должны идти все классы, см. `tools/build-android.sh`),
   шторка ☰, микшер свайпом вверх по доку, Track Settings с вкладками сверху.

Что вошло (портрет, телефон):
- Шапка: ☰ + «Winteria» (+ браузер в плейлисте). Нижний док: undo/redo, название проекта, play/stop, темп.
- Шторка ☰ слева (как в приложении Claude): разделы и действия проекта. Piano убран из навигации, пиано рол открывается из клипа.
- Плейлист: названия дорожек в колонке справа. Микшер шторкой снизу (свайп вверх по доку).
- Track Settings: вкладки-пилюли сверху, страницы на всю ширину.
- Браузер: импорт паков (ZIP/папка), сэмплы сгруппированы по пакам, вёрстка под крупный шрифт.
- FL-стиль 1.0.34 убран, вернули прежний дизайн. Пиано рол не прокручивается сам за курсором.
- Вылет после file picker: `process::exit` в конце `android_main` плюс расширенный `config_changes`.

### Android 1.0.44 (Claude)

- Браузер в нашем стиле и под любой размер шрифта телефона: заголовок с ×, вкладки Sounds|Plugins в ряд, одна кнопка «+ Import» с меню (WAV / ZIP / папка), поиск и строки на всю ширину с обрезкой названия. Пока браузер открыт, колонка названий справа сжимается до цветных полос.
- Канал OTA и сертификат релиза возвращены ChatGPT (см. записку выше).

### Android 1.0.45 (Claude)

- Браузер на всю ширину. Пока тащишь сэмпл, браузер уходит. После броска закрыт, чтобы был виден клип.
- Track Settings в портрете: шторка снизу (свайп вверх по доку или второй тап по имени дорожки), закрывается свайпом вниз, тапом по ручке или по затемнению.
- Микшер: правая боковая панель на всю высоту, открывается свайпом от правого края, закрывается свайпом вправо, кнопкой × или тапом по затемнению.
- Исправлено: «Done» перезаписывал флаг закрытия Track Settings.
- ChatGPT: в официальный релиз ставь версию после последней dev (сейчас 1.0.47+).

### Android 1.0.46 (Claude)

- Правая колонка плейлиста: под дорожками кнопки «+ MIDI» и «+ Audio», ниже плашка Master (громкость, тап открывает микшер на мастере). Свайп по колонке листает дорожки.
- ChatGPT: официальный релиз = 1.0.47+.

### Android 1.0.47

Официальный релиз на базе Dev 1.0.46. Сохраняет новый интерфейс Claude и импорт WAV, ZIP и папок. Из официальной 1.0.34 перенесено распознавание двойного тапа по всей ноте. Плавный курсор и ручная прокрутка Piano Roll взяты из Dev. На телефоне нужно проверить повторный тап по ноте, переход с официальной 1.0.35, импорт и жесты панелей.

### Android 1.0.48

Мобильный Mixer использует вкладки Channels, Effects Chain и Master под заголовком. В Channels доступны дорожки Audio/MIDI и инсерты, их громкость и маршрутизация. Effects Chain показывает слоты выбранного инсерта либо Master. При первом эффекте на дорожке без выхода создаётся маршрут на свободный инсертовый канал. Проверить добавление, обход и удаление эффектов на телефоне.

В Playlist добавлен Grid с шагами Bar, 1/4, 1/8, 1/16, 1/32 и Off. Видимую правую ручку можно тянуть для изменения длины, в меню клипа есть удлинение и укорачивание на шаг. WAV из Browser на Audio создаёт аудиоклип. На MIDI назначает сэмплер и в свободной позиции добавляет клип с нотой на исходной высоте сэмпла. Тап по звуку также может назначить его на выбранную MIDI-дорожку. На Android проверь быстрый перенос из Browser и перетаскивание коротких клипов.

### Android 1.0.49

Отдельная кнопка Grid перенесена в круглое меню справа от шкалы Playlist. В меню раскрываются Playlist (шаг сетки и следование за курсором), Clips (открыть, дублировать и изменить длину выбранного клипа) и Audio (браузер, настройки аудиоклипа и экспорт). Положение меню и действия нужно проверить на телефоне.

### Android 1.0.50

- Portrait Studio retains the Winteria wordmark and puts BPM, metronome, Mic and MIDI in the header.
- The bottom transport adds bar navigation and a scrubber, with Settings and Playlist shortcuts.
- Playlist swipes, sample placement and double-tap clip editing share the original native arrangement. Track Settings groups controls under Main and stacks effects in a separate view.
- The Mic action requests Android microphone permission and records through the native engine. Hardware microphone and touch behavior still need an on-device check.

### Android 1.0.51

- Portrait Studio keeps Menu, Winteria and Browser in the header. BPM, metronome, microphone and MIDI sit in a centered floating panel above the transport dock.
- Browser is accessible from other Studio views through the Playlist. Phone testing is pending.

### Android 1.0.52

- Portrait Studio follows the 1.0.51 layout with the brand label removed from the header.

### Android 1.0.53

- Playlist audio tracks remember the last chosen sound. A tap on an empty grid cell repeats it at the current snap interval. Empty tracks still prompt for a sound.
- The visible 1/32 subdivisions align with the placement grid.
- Horizontal two-finger zoom keeps the Playlist centered at the gesture. BPM, metronome, microphone and MIDI now sit in the Studio header between Menu and Browser.

### Android 1.0.54

- Portrait Studio centers frameless BPM, metronome, microphone and MIDI controls. Menu and Browser sit at opposite edges.

### Android 1.0.55

- The waveform button renders the selected Playlist track off the UI thread. The shared progress animation stays visible during rendering, then Studio selects the rendered WAV on a new audio track and mutes the original.
- Repeated renders use distinct WAV filenames. Rendered tracks keep their full length when reopening a saved project.
- Tap or drag the Playlist ruler to set the playhead. Audio clips resume at the seek position. Hold a clip to duplicate or delete it, and use Undo from Playlist options to reverse the edit.

### Android 1.0.56

- The waveform button renders the entire audible Playlist into one WAV and imports it on a new audio track. The original tracks are muted after a successful render. Desktop retains its single-track command.
- MIDI and audio track cards expose Main, Mute and Solo in their own Playlist menus.

### Android 1.0.57

- Mobile light and dark themes use muted blue-gray surfaces and subtle Morning Dawn tints. Playlist clips keep the existing lane colors with softer fills and dark text. The shared desktop palette is unchanged.
- Browser opens as a left drawer over the Playlist in the same style as the main menu. Dragging a Browser item clears the overlay so it can land on a track.
- New MIDI clips use the active grid step. New audio clips align to the next division when space permits, and clip resizing snaps the right edge. The metronome follows arrangement or pattern position after seeking and looping.

### Android 1.0.58

- Mobile Track Settings opens with Sample or Instrument and has dedicated Controls and Effects tabs. The Main action in Playlist goes directly to Controls.
- Tapping the name of a drum in Steps opens a sound panel with waveform, preview, WAV selection, level and pan.

### Android 1.0.59

- Piano Roll adds fixed controls to extend the current grid by a bar and duplicate the melody to the right. A clip extension stops at the next clip, and copies never overwrite neighboring clips.
- Saved patterns retain valid nonstandard step counts.

### Android 1.0.60

- Piano Roll redraws the updated notes and grid in the same frame after extending or duplicating. A shared pattern can fork without resetting the open clip's scroll position or octave.

### Android 1.0.61

- Mobile Piano Roll has a Select tool with tap selection and drag marquee. Duplicate selection places selected notes to the right at a grid-aligned interval and extends the clip only when necessary.

### Android 1.0.62

- Studio has a scrollable center toolbar and a pencil to draw volume automation on the selected Playlist track. Existing automation plays through the shared engine.
- Home, Projects and Plugin Store are in the mobile hub navigation. Home surfaces recent projects, templates and sounds. Plugin Store catalogs the built-in instruments and effects.

## ➜ ChatGPT: 1.0.63 не запушена

У пользователя установлена официальная 1.0.63, но в `winteria` последняя 1.0.62 (`43b4b6b`), в `pulse-updates` тоже 1.0.62. Запушь исходники 1.0.63 в `winteria` и опубликуй её OTA. Следующие версии Claude будут 1.0.64+ поверх твоего коммита.

## ➜ ChatGPT: Claude тоже работает над официальной версией (решение пользователя)

С этого момента официальную Winteria делаем вдвоём. Что нужно от тебя:

1. **Запушь исходники 1.0.63** в `winteria` (у пользователя стоит 1.0.63, в ветке и в `pulse-updates` только 1.0.62)
   и опубликуй её OTA. Без этого следующие версии Claude будут без твоих изменений 1.0.63.
2. **Ключ релиза.** Передай пользователю **лично** (не в git, не в публичный чат) keystore и пароль. Он добавит их
   секретами в `ffight763-glitch/Vinteria-git3` → Settings → Secrets and variables → Actions:
   `ANDROID_KEYSTORE_B64` (base64 keystore) и `ANDROID_KEYSTORE_PASSWORD`. Тогда CI подпишет сборки Claude тем же
   ключом, и они встанут поверх твоих.
3. Дальше правила из `AGENTS.md`: всегда `git pull --rebase` перед работой, пуш до публикации, версия строго следующая.
   Каждый пишет сюда короткую запись о своей версии.

### Android 1.0.64

- The 1.0.63 source and APK were unavailable. This release starts from the published 1.0.62 code and the shared handoff updates.
- Playlist clips clear pending move and resize state when a pinch takes over or automation drawing is active. A fresh clip drag starts with fresh state.
- Check clip resize, drag, pinch, and drawing on Android. Official builds use the existing OTA signing certificate.

### Android 1.0.65

- Playlist and Piano Roll choose one zoom axis from the initial change in finger spacing and keep that axis for the gesture. Horizontal pinches scale time and vertical pinches scale row height.
- Pinching the Piano Roll suppresses note edits and marquee selection until release. Check diagonal gestures and transitions from one finger to two on Android.

### Android 1.0.66

- Drag a Playlist track by its colored handle onto another track or a folder. Folder cards collapse or expand the grouped rows without altering clip indices or playback. The track dialog includes enable/disable, solo, Main and removal from a group.
- The Mixer has compact tabs and a simpler Master layout. Render joins the scrollable controls in the portrait Studio header.

### Android 1.0.67

- Piano Roll labels the length button “+1 beat” and adds a quarter note of empty grid per tap, capped by the next clip or the pattern length limit.

### Android 1.0.68

- Piano Roll keeps playback, stop and close visible, with recording and full view in Tools. Tools toggles a horizontally scrollable row for grid, octave, zoom, scale, and other editing options. Grid snap persists independently of zoom.
- “+1 beat” extends the selected MIDI clip from either Piano Roll entry point when that clip uses the edited pattern.
- Drag on the Playlist ruler to loop whole bars. Tap still seeks; use Clear loop in Playlist options or hold the ruler to turn looping off. Audio crossing the loop start resumes on each pass.
- Tap or drag the Piano Roll ruler to select notes by beat. Clone shifts the notes by the selected beat range. Tools → Slide marks new notes or toggles glide on an existing note.
- Audio clip settings use touch-sized controls and omit desktop shortcut hints in the mobile sheet. The Piano Roll keyboard and grid use softer theme colors.
- Clone replaces the two main selection and duplication buttons. Select notes remains in Tools. Undo and Redo are in Piano Roll, and individual note and velocity edits register undo steps.

### Android 1.0.69

- Playlist rewinds its viewport on Stop, Return to start and natural song completion. Fresh Play reveals its start point, Pause keeps the viewport, and follow-playhead can track a loop jumping left.

### Android 1.0.70

- Drag from the Playlist track name or handle to create a group or move a track into an existing one. The drag shows a floating track name and highlights the drop target. Track options stay on their own touch target.
- Piano Roll shows an exit button next to Clone while selection is active. It clears the highlighted range and restores note editing.

### Android 1.0.71

- Select an Audio or MIDI clip and drag its top left or right handle to shape Fade In or Fade Out. The right edge below the fade handle still resizes.
- Playlist clips and track cards indicate playback activity. Steps flash on active hits, and Session's running pads pulse with the transport.
- MIDI clips show a note thumbnail below the name, including cropped and repeated content. The Playlist Master card shows output level as a horizontal meter.

### Android 1.0.72

- Swipe a Playlist track name horizontally for track actions, including locking and confirmed removal. Vertical dragging still groups tracks. Lock protects the track's clips and automation from Playlist edits while preserving playback.
- Playlist options can enable or disable all tracks. Playing track cards show a stronger pulse and clip progress.
- Playlist has Select, Clone, Undo and Redo. Tap individual clips or drag a marquee from an empty cell to select them. Clone preserves relative timing and lanes for mixed Audio and MIDI selections.

### Android 1.0.73

- Piano Roll uses its ruler to select a beat-aligned playback loop. Tap seeks, hold or Clear loop clears the range. The older note Select and Clone controls were removed.
- Playlist track actions open Effects. On a dedicated track insert, supported Mix, Cutoff and Feedback parameters offer Create automation clip. The automation row supports drawing, dragging, resizing, cloning and deletion. The engine restores the configured effect parameter outside the clip.

### Android 1.0.74

- Android live playback gets 3 dB of output gain ahead of the existing master limiter. Project levels and offline WAV renders use unity output gain.
- Playback level and limiter behavior still need an on-device listening check.

### Android 1.0.75

- Playlist options export the selected loop to WAV at its exact boundaries, with pre-roll for notes and effects already sounding at its start.
- Select Audio and MIDI clips, then choose Clips → Render selection. The rendered audio is placed at the original start on a new track. It mutes the rendered source clips and supports Undo.
- Playlist labels remain readable when clipped at the viewport edge, and grid subdivisions thin out at dense zoom levels.
- Playlist Select, Clone, Undo and Redo float above the transport at the lower left, leaving the top of the timeline free.
- Landscape Playlist keeps the track cards, track actions, Add track controls and Master on the right. The right column can collapse, and its mixer drawer works in both orientations.
- The landscape left toolbar pins Browser for Playlist and uses the floating toolbar for its history buttons.
- Landscape Piano Roll opens as a bottom sheet with a drag handle and a Full view option.
- Master is a dedicated Playlist lane in both orientations. With the pencil enabled, choose its card or timeline row to edit the master volume curve.

### Android 1.0.76

- Refresh the flat mobile theme in light and dark modes. Improve control press states and the Playlist's visual contrast between clips, track cards and the grid.

### Android 1.0.77

- Keep the Master card in the Playlist sidebar. Only draw the timeline lane when the pencil is active and Master is selected.

### Android 1.0.78

- Master gain and its pencil automation are edited inside the Playlist sidebar card. The central grid stays free of Master UI in both orientations.
- Double-tap a Playlist clip to show its radial action menu. Hold a MIDI clip to open Piano Roll directly.

### Android 1.0.79

- Landscape studio moves transport and section controls from the left sidebar into a collapsible drawer on the right. The chevron stays at the bottom-right corner and opens the drawer upward. Portrait keeps its existing layout.
- Native Android touch testing is pending.
- Landscape Piano Roll places its toolbar on the left in sheet and full view. The note grid, editor playback range and +1 beat honor the MIDI clip length. Track settings open from the bottom in landscape.

### Android 1.0.80

- Move the landscape Piano Roll toolbar to the right in both sheet and full view. Keys, time ruler and note grid occupy the left. Native Android touch testing is pending.
- Move the landscape track settings tabs to a vertical column on the right of the bottom sheet.
- Landscape Playlist track actions open as a scrollable panel at the left edge.

### Android 1.0.81

- The landscape Piano Roll section uses the full grid width without the left Studio sidebar. Playlist and orientation controls are available in the right toolbar.
- While editing a MIDI clip, the Playlist Browser and track actions cannot cover the Piano Roll. Native Android touch testing is pending.

### Android 1.0.82

- A single tap on an Audio or MIDI clip in Playlist opens its radial action menu in normal editing mode. Long-press still opens MIDI directly in Piano Roll, and Select mode still toggles selection.
- Native Android touch testing is pending.

### Android 1.0.83

- Landscape Playlist: Select, Clone, Undo and Redo moved from the floating pill into one bottom dock bar. The studio controls chevron sits at the right end of the same bar and opens the controls column above it. Portrait and full view keep the floating pill; other landscape sections keep the old drawer chevron.
- Native Android touch testing is pending.

### Android 1.0.84

- The Playlist clip action menu is no longer a radial wheel: tapping a clip shows one row of icons (Open, Extend, Shorten, Clone, Delete, Close) just above the tap point, or below it when there is no room. The screen is no longer dimmed; tapping outside still closes it.
- Native Android touch testing is pending.

### Android 1.0.85

- Landscape Playlist: the right-hand studio controls drawer and its chevron are gone. Its controls (Return to start, Play/Pause, Stop, Record, Tempo, Automation pencil, Render, Export and the section buttons) now sit in the bottom dock after Select, Clone, Undo and Redo. The dock scrolls horizontally on narrow screens. Other landscape sections keep the drawer.
- Native Android touch testing is pending.

### Android 1.0.86

- Fix: Playlist clip actions, the track options panel ("…"), the track settings sheet and the landscape Piano Roll sheet could stop responding. Their full-screen dimming catcher shared the Foreground order with the panel; after one tap on the catcher egui moved it above the panel, so every later tap landed on the invisible catcher and only closed the panel. The catchers are now `Order::Middle` (below their panels, like the Browser and Mixer sheets).
- The clip action row has no catcher at all: a press outside the row closes it and still reaches whatever is under the finger.
- Reproduced and verified headlessly by driving the real app (landscape 800×360, touch events): open menu → close by tapping outside → reopen → Delete now works; same open/close cycles for the track sheet and the track options panel; every dock button responds.
- Native Android touch testing is pending.

### Android 1.0.87

- Fix: the tap that closes the clip action row no longer also acts on an empty Playlist cell (before, it made a new MIDI clip or, on an Audio lane, opened the Browser to pick a sample). Taps on clips, track names and buttons still go straight through.
- Headless landscape Playlist run of the real app with touch events (not committed): clip tap → menu; Extend, Shorten, Clone, Delete, Open (piano); Undo after Delete; close menu by tapping outside and reopen; tap empty cell → new clip; drag clip; long-press → Piano Roll, no stale touches; track name select and second tap → settings sheet, Done closes; "…" → track options; ruler tap → seek; dock Select / box selection / Clone; sidebar Browser. All pass.
- Native Android touch testing is pending.

### Android 1.0.88

- Fix (portrait Piano Roll): the toolbar sat in the middle of the screen and the note grid was gone. Since the landscape Piano Roll work the portrait toolbar used a `left_to_right(Center)` layout, which in egui takes the whole remaining height. Portrait now uses wrapping rows at the top again (`piano_toolbar_row`), and Tools is one horizontally scrolling row as before. Landscape keeps the right-hand column.
- Checked visually on a 400×860 virtual screen with the desktop build in mobile mode: Piano Roll (closed and open Tools), Steps, Session, Mixer, Tracks, nav drawer and the track settings sheet in portrait.

### Android 1.0.89

- Mobile Mixer redone FL-style, one page instead of Channels/FX/Master tabs: a horizontally scrolling row of strips (Master first, then Insert 1–8) with a vertical fader, a level meter, dB, Mute/Solo and coloured dots for the tracks routed into each bus. Tap a strip to select it.
- Below the strips, everything about the selected strip: name and pan (inserts), the tracks feeding it with their volume, mute and routing picker, "+ Route a track here", the effects chain, and for Master the EQ and compressor.
- Checked on the desktop build in mobile mode at 400×860 and 860×400: routing MIDI 1 to Insert 1 moves its dot to that strip and lists it under the bus.

### Android 1.0.90

- Tapping the Master card in the Playlist (title or meter) opens the full Mixer page, the same one as in the main menu, with Master selected. Before, the meter half only took drags and the title opened the old side sheet. Dragging the meter still sets the Master volume.

### Android 1.0.91

- Mobile Mixer laid out like FL Studio's mixer: a header (Mixer · selected channel, FX / Route / EQ), the channel strips on the left (Master + Insert 1–8: number box, vertical name, green LED mute, pan knob, stereo meter, square fader, coloured dots for the tracks feeding it; double-tap pan or fader resets) and the dock of the selected strip on the right.
- Dock FX: ten FL-style slots (name, wet knob, on/off LED); an empty slot opens the effect list, a filled one expands Remove, Automate (for a track's own insert) and the effect controls. Route: the tracks playing through the strip with routing picker, volume and mute, "+ Route a track here", and the bus name. EQ (Master): the master EQ and compressor.
- Checked on the desktop build in mobile mode at 400×860 and 860×400.

### Android 1.0.92

- Phone Browser laid out like Ableton's: search with an Import (+) button on top; left column CATEGORIES (Sounds, Drums, Instruments, Effects) and PLACES (each sample pack and the loose Library files, with counts); right column the items with ▷ to preview a sample (tap again to stop) and a drag grip; "Tap to: …" destination at the bottom. The drawer is wider (96% of a portrait screen, 62% in landscape) to fit both panes. Drums = samples whose name/path looks like a drum (kick, snare, hat, …).
- Drag a sound straight onto a Playlist track from anywhere on its row: a sideways drag picks it up, an up/down drag scrolls the list. No need to select a track first.
- Checked on the desktop build in mobile mode at 400×860: dragging Factory Kick by its grip onto Audio 1 and Factory Perc by the row onto Audio 2 creates the clips.

### Android 1.0.93

- Track settings → Instrument on the phone: instead of small menus, four big actions with a line on what each does: Choose instrument, Load WAV, Presets, Open editor. The first three open a left side panel: instruments with what each is for (✓ on the current one), samples with search and the preview triangle (the pick becomes a Sampler), presets (stay open to compare; the last one is highlighted). Tapping outside or × closes it; it always sits above the sheet.
- Open editor now works on the phone: the PC plugin window (preset library + engine knobs) full screen with Done. Before, it set a flag only the PC drew.
- Checked on the desktop build in mobile mode at 400×860: Poly Synth chosen, Soft Pad applied (Tone 5000 → 1800), Factory Hi-Hat loaded as 808 Sampler, editor opened and closed.

### Android 1.0.94

- Phone instrument editor (`instruments_ui::show_mobile`): two tabs, Sounds (the preset library with category chips and 44 px rows showing category · engine) and Edit (the engine). In Edit the engine panels use a phone layout: knob rows wrap (`row`), fixed panel widths shrink to the screen (`fit_w`), and Wavetable's side-by-side boxes stack (`row_top`). The PC editor is unchanged (the helpers only switch while `inst_mobile_layout` is set).
- Checked on the desktop build in mobile mode at 400×860: Poly Synth, Wavetable, Analog, 808 Bass and 808 Sampler editors fit the width; the Sounds list applies presets.

### Android 1.0.95

- Piano Roll Oct − / Oct +: they always moved the view by an octave, but on an empty grid only the small key labels changed, so they looked dead. Now the octave in view is shown between the buttons (C3, C4…, same naming as the keys) and the octave that comes into view glows for a moment. Limits 0–9 are respected.
- Checked on the desktop build in mobile mode (portrait and landscape): Oct + moves B3→B4 with the glow, Oct − moves back.

### Android 1.0.96

- Phone audio watchdog (`AudioIo::keep_alive`, every frame): when the output stream errors out (audio focus lost, Bluetooth/headphone switch, back from the background) or stops calling back for 1.5 s, the default output is opened again (at most every 2 s) and a playing transport carries on from its position. Before, the stream was just dropped: no sound until the app restarted, and Play flipped back because the engine never got it.
- Android buffer 512 → 1024 frames (~21 ms) by default; Settings → Sound has Fast (512) / Stable (1024) / Extra safe (2048), Restart audio, and a line with running buffers and reconnects for diagnosis.
- Diagnosis: a headless end-to-end run (phone UI → shared → the real audio callback, not committed) passed: Playlist clip sounds, pause keeps the place, play resumes, stop rewinds, quick play/pause, Piano Roll plays and loops only the clip in pattern mode, Playlist after Piano Roll starts the song from the start, no silent buffers while editing during playback. So the reported silence / play-stop trouble points at the Android output stream, which this addresses.

### Android 1.0.97

- Fix: Piano Roll Chords with no key chosen (Chromatic, the default) stacked every other semitone and put a cluster (F#-G#-A#) instead of a chord. Chromatic now builds major chords (root, +4, +7, +11 for four notes); keyed scales are unchanged. Regression test `theory::chord_tests`.
- Headless Piano Roll run of the real app with touch input (not committed), portrait and landscape: open on a clip, tap adds a note, single tap keeps it, double tap deletes, Undo / Redo, drag moves in time and pitch, right edge resizes, Grid cycles the snap, Zoom −/+, Oct −/+, Slide toggles a note, Chords, Loop on/off, velocity lane, +1 beat, ruler drag → loop range and Clear loop, Rec + side key records, Play / Stop, Full view, Done. All work; only the chromatic chord was wrong.
- Usability notes (not changed): in landscape every control lives in one scrolling column, so after opening Tools the Done/Play buttons scroll away; in portrait the Tools row scrolls sideways (Zoom, Key, Chords, Loop are off screen at first); the velocity lane is off by default in landscape.

### Android 1.0.98

- Steps laid out like FL Studio Mobile's step sequencer: one row per drum with the name block on the left (selected one in the drum's colour, the loaded sample's name under it; tap selects and previews, hold opens the sound sheet), the steps in a line on the right in groups of four, a sideways swipe scrolls them (own scrolling: taps land exactly on the cell). Top: pattern, a SWING knob (drag, double-tap resets), tabs Steps / Mixer / Sample. Mixer shows Level and Pan knobs and M per row; Sample shows REV, Start and Fade (the processed copy is rebuilt from the original like the PC sample window). Under the rows a graph editor (collapsible) edits Velocity / Pan / Pitch / Chance per step of the selected row (double-tap resets). Hold a step for the Smart Step sheet as before.
- Checked on the desktop build in mobile mode at 400×860: steps toggle on the tapped cell, swipe scrolls, velocity bar edit, Mixer and Sample tabs.

### Android 1.0.99

Piano Roll, ideas from FL Studio's PC Piano Roll:
- Note names on the notes also on short ones (from 16 px wide, size follows the row height).
- Event editor under the grid: tap the label on the left to switch Velocity → Pan → Chance (pan bars from the middle line, double-tap resets); a splitter handle above it resizes it (44–320 px, per orientation).
- Ghost notes: the notes of other MIDI clips that sound at the same time (any track) are drawn faded, outlined in their track colour, not editable.
- Landscape: Done / Play / Stop / Full view / Tools stay at the top of the side column; only the tools under them scroll.
- Zoom bar over the ruler like FL's: drag the middle to scroll, drag an end to zoom in time (the other end stays put).
- Checked on the desktop build in mobile mode (portrait and landscape). Note: a mouse that jumps between clicks drags the grid's ScrollArea (desktop/mouse only); with touch input every tap lands on its row (headless check).

### Android 1.0.100

- Refine the existing Pulse mobile style in both themes: lightly translucent transport, clip actions and landscape Piano Roll toolbar, softer Playlist clips and matching lavender notes and event bars.
- UI geometry and editing gestures are preserved. Visual verification on Android is pending.

### Android 1.0.101

- Bring Material 3-inspired controls into the Pulse mobile interface. Hub navigation uses icons, captions and a tonal pill for the selected destination. Shared mobile buttons have rounded outlines and hover, focus and pressed states.
- Transport and editor gestures are unchanged. Native Android visual testing is pending.

### Android 1.0.102

- Improve Playlist grid contrast at bar, snap and track boundaries in both themes.
- Dev builds check their own repository's update manifest and reuse the Android in-app installer. CI publishes the manifest after the signed APK and checks signer continuity against the previous manifest. Official builds keep their existing feed.
- Native Android installation and touch testing are pending.

### Android 1.0.103

- Playlist and Piano Roll share grid fractions from Bar through 1/32, including 1/2. Piano Roll offers direct grid selection and draws the snap divisions used to place notes. At wide zoom both views thin their grid while keeping the chosen snap.
- Native Android grid and Dev OTA installation testing are pending.

### Android 1.0.104

- Group mobile clip settings into source, playback and actions. Audio settings show a playable waveform, and MIDI settings open from the Playlist clip action bar alongside Piano Roll.
- Browser effects, including signature plugins, can be dropped directly on Audio and MIDI track headers, as well as on their Playlist rows. Instruments can be dropped on MIDI headers and MIDI rows.
- Native Android interaction testing is pending.

### Android 1.0.105

- Piano Roll has a Select mode on the ruler. Tap a beat or drag over multiple beats, then Copy → duplicates their notes into the next beat range. Copy can extend the clip when space permits, and forks shared patterns before editing.
- Portrait Piano Roll puts transport and tools in one horizontally scrolling toolbar. Loop selection remains available when Select is off.
- Android touch verification is pending.

### Android 1.0.106

- In Piano Roll Select mode, drag on empty grid to select notes with a marquee. Drag a selected note to move the group in time and pitch, with grid snap and a live preview. The selection has a contrasting outline in both themes. Ruler selection and Copy → remain available.
- Native Android touch verification is pending.

### Android 1.0.107

- Playlist bar lines stand out from lighter subdivisions. Dense grids thin earlier with the chosen snap preserved, and the selected lane has a softer tint.
- Portrait Playlist puts Select, Clone and history in the bottom panel above transport. The header, landscape dock and full-view tools use the same frosted surfaces.
- Native Android visual and gesture verification is pending.

### Android 1.0.108

- New project saves identify as WINTERIA and use the Winteria Projects folder. The loader still accepts SYNTHESIA project files, and the hub lists projects from the old folder without moving them.
- The project round-trip test covers both identifiers. Native Android save verification is pending.

### Android 1.0.109

- Redesign the portrait Studio dock with round icon buttons, a position slider and Playlist options in the dock. The current section icon opens a menu for Playlist, Drum Rack, Piano Roll, Session, Mixer and Tracks. The landscape dock keeps its layout.
- Native Android visual and touch verification is pending.

### Android 1.0.110

- Portrait Playlist selects on a tap, opens MIDI Piano Roll or audio settings on a double tap, and opens a labelled action grid on hold. Landscape retains its existing clip menu and gestures.
- Refresh portrait Track Settings with a frosted header, rounded tabs, a compact MIDI instrument action grid and an Audio sample card.
- Native Android touch and layout verification is pending.

### Android 1.0.111

- The portrait Studio dock replaces the top header. Browser, BPM, metronome, automation, Mic, MIDI and render remain available via the dock's Studio menu.
- Portrait Playlist moves track cards and Master to a collapsible left rail. Tap or swipe its Tracks handle to expand or collapse, retaining timeline alignment, track actions and drop targets. Landscape keeps its right rail.
- Native Android touch and layout verification is pending.

### Android 1.0.112

- Replace Drum Rack pattern chips with a grid selector and bind edits to a Playlist MIDI clip. First step entry creates a clip on the selected unlocked MIDI track when there is room, otherwise on a dedicated Drum Rack track. To Audio uses the existing selection bounce.
- Drum step grid supports Bar, 1/2, 1/4, 1/8 and 1/16. Coarser views keep the hidden intermediate steps intact.
- Native Android verification is pending.

### Android 1.0.113

- Simplify the portrait dock section menu to Playlist, Drum Rack, Session and Mixer. Piano Roll remains available from a MIDI clip.
- Native Android touch verification is pending.

### Android 1.0.114

- Replace portrait Playlist's side rail with a horizontally scrolling row of track cards above the dock. The timeline uses the full width. Track actions, plugin drops, grouping, Master and Add track remain on the strip.
- Remove section shortcuts from the portrait Studio drawer. They are available from the dock section menu.
- Native Android touch and layout verification is pending.

### Android 1.0.115

- Polish the portrait track strip with edge padding, automatic reveal when the active track changes and a slim draggable position indicator.
- Present portrait track actions in a frosted bottom sheet with a drag handle and the existing actions and confirmation.
- Native Android touch and layout verification is pending.

### Android 1.0.116

- Give the existing mobile dark theme a Chat Pulse-inspired graphite-and-silver palette. Graphite Silver uses the Chat Pulse night accent on transport and selection controls, with charcoal cards and neutral grid lines. Light colors and the clip and note palettes are preserved.
- Native Android visual verification is pending.

### Android 1.0.117

- Restore the portrait top bar for BPM, recording, automation and Browser. Project actions open from the top-bar menu instead of the left drawer.
- Remove the duplicate project-menu button from the dock and retain the transport and section selector there.
- Native Android visual and touch verification is pending.

### Android 1.0.118

- Add a vertical Playlist for portrait: tracks are columns and bars run down the screen. The playhead is horizontal. Move clips across tracks or down the timeline, resize from the lower edge, and use the existing clip actions.
- Playlist options can switch back to a horizontal timeline. Landscape and volume automation editing continue to use the horizontal view.
- Native Android visual and gesture checks are pending.

### Android 1.0.119

- The portrait project actions open in an animated left drawer from the top-left button. Quick controls and Browser stay in the top bar, and sections remain in the dock.
- Native Android visual and gesture verification is pending.

### Android 1.0.120

- Remove duplicate track headers above the portrait vertical timeline. Track cards remain in the strip above the dock. Full view keeps a small space for its Playlist options button.
- Native Android layout verification is pending.

### Android 1.0.121

- Drain retired audio project snapshots in the mobile update loop. Vertical Playlist uses direct track-column lookup and limits MIDI thumbnail drawing to visible, distinguishable marks.
- The reported Android freeze has not been reproduced locally. Native-device verification is pending.

### Android 1.0.122

- Mobile Playlist no longer follows the playhead in either orientation. Manual Stop and Return to start can still reset the view.
- Android song playback stops and returns the playhead to the start at the end unless an explicit loop is enabled. Native Android playback verification is pending.

### Android 1.0.123

- Restyle mobile Session as a four-by-four pad launcher using the Playlist's graphite surfaces, colored track headers and pastel clips. Switch track and scene banks from the top controls. Scene launches, queued states and active pads are visible in the matrix.
- Empty pads accept the selected compatible Playlist clip; MIDI pads also accept the current pattern and Audio pads can use the track's last sample. Native Android interaction tests are pending.

### Android 1.0.124

- Make Track settings explicit in the Audio and MIDI card actions menu, and open it directly with a double tap on a portrait track card. It opens on Sample or Instrument by default.
- Native Android interaction testing is pending.

### Android 1.0.125

- Rebuild the mobile Track settings source page around source, playback and waveform sections. Audio exposes sample replacement, gain, reverse, pitch and fades. MIDI retains instrument selection and adds track playback controls above the Sampler waveform.

### Android 1.0.126

- Replace the four-tile MIDI instrument panel with a source selector, a compact action row, and rotary instrument sound controls in the same visual style as Audio Track settings.

### Android 1.0.127

- Put source and playback into two compact columns in mobile Track settings. Use bounded pitch and level controls and keep the sample waveform full-width under the panel.

### Android 1.0.148

- Portrait vertical Playlist and Drum Rack look: column headers tinted with the track colour (stronger when selected) with a 2–3 px underline; clips get a soft sheen (light top → darker bottom) and a track-colour edge on the left (`clip_shine`); lit Drum Rack steps glow in their drum's colour.
- Clip contents: MIDI clips show their real notes (time runs down the clip, length = note length, spread over the clip's pitch range); drum clips show their lit steps as short bars per drum row; audio clips show a mini waveform down the clip (respects offset, warp tempo and reverse).
- Checked on the desktop build in mobile mode (400×860) with the Trap Beat (808 Mafia) template: drum, 808 bass, wavetable and analog clips.

### Android 1.0.149

- Home: Library above Recent. The bento tiles and recent-project carousel retain their layout.
- Mobile theme: translucent grey chrome, with a soft highlight on the header, navigation, studio sidebars and transport docks.
- Portrait Playlist: audio clip settings open from the left with a translucent grey finish, a fixed header and scrollable controls.
- Playlist: MIDI clips now honor Clip gain (up to +6 dB) and Poly Synth defaults to a 3.1 dB higher output. Engine measurements confirmed that the WAV level is unchanged.
- Android handset verification pending.

### Android 1.0.150

- Plugin Store: tap a built-in instrument to add it to a new or selected MIDI track. Tap an effect to add it to the selected track’s mixer insert. Both open the corresponding track settings page.
- Android handset verification pending.

### Android 1.0.151

- Portrait studio: Views and Playlist options open as bottom sheets. Clip actions open in a left sidebar.
- Android handset verification pending.

### Android 1.0.152

- Portrait Playlist: subtler glass highlights and track-tinted clip bodies, with stronger note, waveform, and title contrast.
- Playlist options: full-width actions, an explicit timeline state, and the current grid value on the right.
- Clip actions: narrower sidebar and a separated red Delete action. Auto Play emphasizes its caption while active.
- Android handset verification pending.

### Android 1.0.153

- Home: added a vertical project-card list beneath Recent. Leaving a project returns to Home.
- Android handset verification pending.

### Android 1.0.154

- Portrait Drum Rack: pad settings open from the left and WAV search opens from the right. Choosing a sample returns to the same pad.
- Android handset verification pending.

### Android 1.0.155

- Home: stronger section headings, more contrast across Library and Recent tiles, and compact project list rows.
- Portrait studio: Views now opens from the left.
- Android handset verification pending.

### Android 1.0.156

- Portrait Playlist options now open from the left with a fixed header and a full-height scrolling list.
- Web: See all in Recent shows saved web projects.
- Android handset verification pending.

### Android 1.0.157

- Added Winteria as the default mobile theme, using the launcher's violet ring, liquid fill and dark background. Graphite Silver and Day remain selectable.
- Android handset verification pending.

### Android 1.0.158

- Mobile surfaces are opaque with no glass sheen.
- Projects can import editable .wd, .vpd and .synthesia files from the Android file picker. Imports keep the original and receive a distinct name if necessary.
- Built-in project folders remain discoverable with a sync folder configured, and their projects save beside their linked media.
- Android handset verification pending.

### Android 1.0.159

- Added Dark Graphite with muted warm-white controls and Toxic Dark with lime accents. Existing mobile themes and preferences remain available.
- Android handset verification pending.

### Android 1.0.160

- Added 18 factory sounds across Wavetable, Analog and 808 Bass. The sounds are available in the preset browser and in instrument editors.
- Android handset verification pending.

### Android 1.0.161

- Track preset picker now plays a short dry preview independently of the Playlist.
- Revoiced Drift Cowbell and Glass Current, strengthened Aurora Choir, corrected Orbital Lead oscillator tuning and pulled back the six new 808 presets.
- Android handset verification pending.

### Android 1.0.162

- Graphite Silver now uses brighter silver controls, cooler slate panels, clearer text and more visible selection and grid colors.
- Android handset verification pending.

### Android 1.0.163

- Mobile Playlist clips highlight and pulse in sync with the beat. Both orientations show progress, and muted clips and inactive MIDI steps stay unlit.
- Android handset verification pending.

### Android 1.0.164

- Track Effects opens a scrollable bottom sheet to add a device on mobile.
- Android handset verification pending.

### Android 1.0.165

- Effect preset menus use the mobile bottom sheet. Filter type labels reflect the filter mode, and the device distinguishes FX Mix from Slot mix.
- Piano Roll now fits about one bar in portrait, shows beat guides when snap is Off, and gives short notes a separate resize edge. Ctrl/Cmd+A selects all editable notes and arrow keys nudge the selection.
- Android handset verification pending.

### Android 1.0.166

- Moved mobile device actions into an explicit button so the full-card context menu no longer intercepts taps on effect controls or presets.
- Android handset verification pending.

### Android 1.0.167

- Phonk templates with a cowbell hook (Drift Phonk Hard, Phonk House 2, Memphis Phonk) no longer go silent when the
  Phonk pack is missing (it is local only, not in git / the APK): the hook stays on the Analog synth unless the cowbell
  file really loads (`templates::cowbell_sample_path`).
- Tests that need the local-only packs (808 Mafia, Phonk Kit, Yeat KanKan Kit) skip when they are absent; new test
  `cowbell_hook_stays_on_analog_without_the_pack`. `cargo test --release`: 117 passed.
- Android handset verification pending.
