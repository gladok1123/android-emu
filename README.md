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

Всё это — один `.exe` примерно на **700 КБ**, без DLL, без установщика,
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

5. **Включите ассоциацию `.apk`** (один раз):

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
* **ARM-приложения** (`armeabi-v7a`/`arm64-v8a` без x86-кода) внутри x86-Android
  не запустятся. Это ограничение самого Android-x86, а не оболочки. Помогает
  `libhoudini`/`libndk` в некоторых сборках BlissOS.
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
android-emu --install <file.apk> [--size 540x960] [--gpu angle] [--no-launch]
android-emu --run <app> [--detach]     запустить приложение (как ярлык)
android-emu phone [--size 480x880]     просто Android, как телефон
android-emu list | info <app>          список и детали
android-emu remove <app>               удалить приложение и ярлыки
android-emu config <app> ...           окно, экран, GPU, режим
android-emu image list|add|use|remove|catalog|get
android-emu setup | assoc on|off|status
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

* проброс файлов и буфера обмена (drag & drop APK прямо в окно);
* кнопки навигации Android рядом с окном эмулятора;
* скриншоты и запись экрана через `adb`;
* несколько VM-профилей (работа/игры) с разными образами;
* перетаскивание APK на ярлык лаунчера.

---

## Лицензия

MIT. Android-x86, BlissOS, PrimeOS и QEMU распространяются под своими
лицензиями; проект их не включает, а только запускает.
