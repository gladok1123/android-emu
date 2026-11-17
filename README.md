# android-emu

Лёгкая оболочка-эмулятор Android для **Windows** (x86_64). Работает поверх QEMU и
**любого вашего образа Android-x86 / BlissOS / PrimeOS** — ISO или диска.

Что умеет:

* **Двойной клик по APK → установка.** APK разбирается (манифест, `resources.arsc`,
  иконка), копируется в хранилище, на Рабочий стол и в меню «Пуск» кладётся
  обычный ярлык `.lnk` с настоящей иконкой приложения.
* **Ярлык = запуск приложения.** Открывается окно Android, APK автоматически
  устанавливается в гостя через `adb` и запускается его launcher-activity.
* **Размер окна настраивается** для каждого приложения отдельно (`--size`,
  `--resolution`, `--dpi`, `--mode window|fullscreen|headless`, `--gpu`).
* **Один общий Android на все приложения** — второе приложение открывается
  мгновенно, без повторной загрузки системы.
* **Свой образ**: `android-emu image add мой.iso` (или `.img` / `.qcow2`), либо
  скачивание из каталога: `android-emu image catalog`.
* **Режим «просто телефон»**: `android-emu phone --size 480x880`.

Плюс **полноценный интерфейс** (нативное окно Win32):

* установка Android-системы из образа прямо в окне (файл-диалог, копирование,
  выбор активного образа, каталог для скачивания);
* настройки эмулятора: RAM, ядра, ярлыки, пути к QEMU/adb, **размер окна,
  DPI, GPU и режим для каждого приложения**;
* **кнопка «Включить Windows Hypervisor Platform»** — запускает `dism` с
  запросом прав администратора и подсказывает перезагрузиться;
* диагностика системы одной кнопкой;
* **ARM-приложения**: опция ARM-трансляции (libhoudini/libndk), команды
  `android-emu arm status|enable|install|check`.

Всё это — один `.exe` примерно на **730 КБ**, без DLL, без установщика,
**без единой внешней зависимости** (даже JSON, DEFLATE, ZIP, AXML и ARSC-парсеры
написаны внутри). QEMU и `adb` — опциональные внешние компоненты.

---

## Быстрый старт (Windows)

1. **Скачайте и положите `android-emu.exe`** в удобную папку, например
   `C:\Tools\android-emu\android-emu.exe`.

2. **Поставьте QEMU** (нужен `qemu-system-x86_64.exe`):

   * официальная сборка для Windows: <https://qemu.weilnetz.de/w64/>
   * либо `winget install SoftwareFreedomConservancy.QEMU`
   * либо просто распакуйте портативную сборку в
     `%LOCALAPPDATA%\android-emu\qemu\` — лаунчер найдёт её сам.

3. **Поставьте `adb`** (platform-tools), чтобы APK автоматически попадали в
   Android. Достаточно положить `adb.exe` в `%LOCALAPPDATA%\android-emu\adb\`
   или добавить в `PATH`.

4. **Добавьте образ Android** — свой файл:

   ```bat
   android-emu image add D:\Images\android-x86_64-9.0-r2.iso --use
   ```

   или из каталога:

   ```bat
   android-emu image catalog
   android-emu image get "Android-x86 9.0-r2 (live ISO)"
   ```

5. **Откройте интерфейс** (двойной клик по `android-emu.exe` без аргументов):
   вкладка «Android система» → «Добавить образ с диска…», затем вкладка
   «Диагностика» → «Включить Windows Hypervisor Platform».

5b. Либо **включите ассоциацию `.apk`** (один раз):

   ```bat
   android-emu setup
   ```

   После этого **двойной клик по любому APK** = установка + ярлык + запуск.

6. Проверка состояния: `android-emu doctor`.

---

## Как это выглядит в работе

```
Двойной клик по WhatsApp.apk
        │
        ├─ разбор APK: package=com.whatsapp, label=WhatsApp, icon=res/mipmap-xxxhdpi/...
        ├─ копия в %LOCALAPPDATA%\android-emu\apps\WhatsApp - com.whatsapp\app.apk
        ├─ icon.ico (экстракт PNG из APK → .ico)
        ├─ ярлык «WhatsApp.lnk» на Рабочем столе и в меню Пуск
        └─ запуск: QEMU (ваш Android) → adb install → am start -n com.whatsapp/.Main

Ярлык «WhatsApp»
        │
        ├─ если Android уже запущен → мгновенно стартует activity
        └─ иначе поднимает VM с окном нужного размера и потом стартует activity
```

---

## Настройка окна и экрана

```bat
android-emu config "WhatsApp" --size 405x720       :: размер окна и экрана
android-emu config "WhatsApp" --resolution 1080x1920
android-emu config "WhatsApp" --dpi 240
android-emu config "WhatsApp" --mode fullscreen     :: window | fullscreen | headless
android-emu config "WhatsApp" --gpu angle           :: swiftshader | angle | guest | off
android-emu config "WhatsApp" --extra-arg=-smp --extra-arg=4
android-emu config "WhatsApp"                     :: показать текущий конфиг
```

Или сразу при установке:

```bat
android-emu --install app.apk --size 540x960
```

Конфиг каждого приложения лежит в
`%LOCALAPPDATA%\android-emu\apps\<app>\config.json` — это обычный JSON,
его можно править руками.

---

## Свой образ Android

Поддерживается всё, что грузится в QEMU:

| Тип | Файл | Изменения сохраняются |
|-----|------|----------------------|
| Live ISO | `.iso` | нет (чистая загрузка каждый раз) |
| Диск | `.img` / `.qcow2` | да |

```bat
android-emu image add C:\Images\blissos.iso --name bliss
android-emu image use bliss
android-emu image list
```

Если у вас **уже установленный** Android (например, вы сделали raw-образ
`dd`-ом с флешки), просто добавьте `.img` — он будет использоваться как есть, со
всеми вашими приложениями и настройками.

---

## ARM-приложения (важно)

Android-x86 сам по себе **не умеет** исполнять ARM-код. Чтобы запускать
ARM-only приложения (большинство игр, многие банковские приложения), нужен
транслятор внутри образа:

| Образ | Что внутри | ARM работает |
|-------|-----------|--------------|
| BlissOS 14+ | libhoudini / libndk | да, из коробки |
| PrimeOS | libndk | да, из коробки |
| Android-x86 9.0 (официальный) | обычно нет | нужно установить |
| Чистый AOSP-x86 | нет | нет |

В лаунчере есть опция **«Поддержка ARM-приложений»** (вкладка «Настройки»,
включена по умолчанию). При запуске ARM-only APK эмулятор:

1. определяет по содержимому APK, что в нём только ARM-код
   (`.so` в `lib/armeabi*` и нет `lib/x86`);
2. спрашивает у гостя `ro.product.cpu.abilist`;
3. если трансляции нет — пытается активировать её: `enable_nativebridge`
   (libhoudini), `persist.sys.nativebridge` (libndk);
4. если не получилось — подсказывает, что сделать (взять BlissOS или
   поставить модуль).

Вручную:

```bat
android-emu arm check D:\Games\game.apk        :: есть ли в APK x86-код
android-emu arm status                          :: что умеет запущенный Android
android-emu arm enable                          :: включить трансляцию в госте
android-emu arm install libndk_translation.zip  :: поставить модуль через Magisk (нужен root)
```

Без ARM-трансляции приложение установится, но при запуске закроется с
`INSTALL_FAILED_NO_MATCHING_ABIS` либо «Приложение остановлено».

## Интерфейс

Двойной клик по `android-emu.exe` (или `android-emu gui`):

* **Приложения** — список установленных APK с иконками, «Установить APK…»,
  «Запустить», «Удалить», «Открыть Android (как телефон)», размер окна/DPI/
  GPU/режим для выбранного приложения.
* **Android система** — добавить образ с диска (ISO/IMG/QCOW2), сделать
  активным, удалить, посмотреть каталог образов.
* **Настройки** — RAM, ядра, ярлыки, поддержка ARM, пути к QEMU и adb,
  «Связать .apk с эмулятором».
* **Диагностика** — отчёт о системе + кнопка включения Hypervisor Platform.

## Что уже работает и что нет

Работает:

* разбор APK (манифест, подписи не проверяются, `resources.arsc`, adaptive icons,
  локализованные названия, permission-список);
* установка приложений в гостя через `adb install`;
* запуск нужной activity (`am start -n`);
* ярлыки Windows с иконками, ассоциация `.apk`, удаление приложений;
* QEMU-сессия: `q35`, `virtio-net` + проброс adb, `usb-tablet` (тач), EDID-размер
  экрана, опциональный звук, монитор QEMU для мягкой остановки;
* авто-фолбэк в «безопасный режим» (без звука, TCG), если QEMU падает на старте;
* диагностика: `doctor`, логи в `%LOCALAPPDATA%\android-emu\logs\`.

Ограничения (честно):

* **Нет аппаратной виртуализации — будет медленно.** Для нормальной скорости
  включите Windows Hypervisor Platform (см. ниже). Без него — программная
  эмуляция (`tcg`), которая грузит Android минуты.
* **ARM-приложения** требуют транслятор в образе (libhoudini/libndk). Лаунчер
  умеет его включать и подсказывает, чего не хватает, но «магии» без образа с
  транслятором не бывает — см. раздел «ARM-приложения».
* **Игры с античитом/DRM** (SafetyNet/Play Integrity) и приложения, требующие
  Google Play, могут не работать без отдельной сборки образа с GApps.
* Проброс файлов/буфера обмена между Windows и Android пока не сделан — в
  планах (см. раздел «Дальше»).

### Ускорение (важно)

```bat
:: от администратора, затем перезагрузка
dism /online /enable-feature /featurename:HypervisorPlatform /all /norestart
```

После этого `android-emu doctor` покажет `acceleration: Windows Hypervisor Platform`,
и загрузка ускорится в 10–20 раз. Если включён Hyper-V, конфликтов с QEMU нет —
WHPX как раз для этого и нужен.

---

## Команды

```
android-emu                       открыть интерфейс настроек
android-emu --install <file.apk> [--size 540x960] [--gpu angle] [--no-launch]
android-emu --run <app> [--detach]     запустить приложение (как ярлык)
android-emu phone [--size 480x880]     просто Android, как телефон
android-emu list | info <app>          список и детали
android-emu remove <app>               удалить приложение и ярлыки
android-emu config <app> ...           окно, экран, GPU, режим
android-emu image list|add|use|remove|catalog|get
android-emu setup | assoc on|off|status
android-emu arm status|enable|install|check   ARM-приложения
android-emu doctor                     диагностика
android-emu adb <любые аргументы>       adb к запущенному эмулятору
```

---

## Структура проекта

```
src/
  main.rs          точка входа
  cli.rs           команды и справка
  installer.rs     APK → приложение Windows (установка, ярлыки, запуск)
  coordinator.rs   один общий Android на все приложения (lock-файл, adb)
  qemu.rs          поиск QEMU, командная строка, сессия, adb-клиент
  config.rs        settings.json и реестр образов
  store.rs         база установленных приложений, конфиг на приложение
  apk.rs           высокоуровневый разбор APK (label, icon, activity)
  axml.rs          бинарный XML (AndroidManifest.xml и все layout-ы)
  arsc.rs          resources.arsc (строки, иконки, локали, плотности)
  zip.rs           ZIP-контейнер APK
  inflate.rs       DEFLATE/zlib (свой, ~500 строк, с тестами на zlib-векторах)
  png.rs           генерация/масштабирование PNG, процедурные иконки
  json.rs          JSON без serde
  paths.rs         %LOCALAPPDATA%\android-emu и служебные папки
  remote.rs        каталог образов и загрузка через curl.exe
  arm.rs           ARM-трансляция: определение ARM-only APK, libhoudini/libndk
  win/gui.rs       нативное окно: вкладки, установка образа, настройки, диагностика
  win/window.rs    поиск и изменение размера окна QEMU (реальный размер экрана)
  win/elevate.rs   запуск с правами администратора (UAC) для WHPX
  win/             Win32 FFI, .lnk (IShellLink/IPersistFile), реестр, консоль
catalog/images.json  список готовых образов
tests/               фикстуры (реальные APK) и векторы zlib
```

Зависимостей в `Cargo.toml` **нет вообще** — `cargo build --release` собирается
офлайн на чистой машине с Rust ≥ 1.75.

---

## Сборка

На Windows (MSVC toolchain):

```bat
git clone https://github.com/gladok1123/android-emu
cd android-emu
cargo build --release
:: результат: target\release\android-emu.exe (~700 КБ)
```

Кроссплатформенно (Linux → Windows, mingw-w64):

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Скрипты: `scripts\build-windows.cmd`, `scripts/build-windows.sh`.
Тесты (работают и на Linux/macOS, и на Windows):

```bash
cargo test
```

---

## Устранение неполадок

| Симптом | Что делать |
|---|---|
| `QEMU was not found` | `android-emu setup` покажет, куда положить `qemu-system-x86_64.exe` |
| Android грузится очень долго | включите WHPX (см. «Ускорение»), проверьте `doctor` |
| APK установился, но приложение не открылось | `android-emu --run "<app>"` и смотрите `%LOCALAPPDATA%\android-emu\logs\*.log` |
| `adb: device offline` | подождите загрузки Android, затем `android-emu adb connect 127.0.0.1:5600` |
| QEMU падает сразу на старте | лаунчер сам повторит в безопасном режиме; лог в `logs\` |
| Приложение требует ARM | нужен образ с libhoudini/libndk (BlissOS) или x86-версия APK |
| Двойной клик по APK открывает «Выбор приложения» | `android-emu assoc on`, затем «Всегда использовать это приложение» |

---

## Дальше (планы)

* drag & drop APK прямо в окно интерфейса;
* кнопки навигации Android рядом с окном эмулятора;
* автоматическое скачивание и установка libndk-модуля из каталога;
* скриншоты и запись экрана через `adb`;
* несколько VM-профилей (работа/игры) с разными образами;
* перетаскивание APK на ярлык лаунчера.

---

## Лицензия

MIT. Android-x86, BlissOS, PrimeOS и QEMU распространяются под своими
лицензиями; проект их не включает, а только запускает.
