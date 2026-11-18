using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using AndroidEmu.Core;

namespace AndroidEmu
{
    internal sealed class MainForm : Form
    {
        private static readonly Color Background = Color.FromArgb(244, 247, 250);
        private static readonly Color Ink = Color.FromArgb(22, 35, 48);
        private static readonly Color Muted = Color.FromArgb(72, 91, 107);
        private static readonly Color Dark = Color.FromArgb(20, 34, 48);
        private static readonly Color Mint = Color.FromArgb(137, 235, 200);
        private static readonly Color Accent = Color.FromArgb(15, 102, 88);

        private readonly SettingsStore store;
        private readonly ICommandRunner runner = new CommandRunner();
        private AppSettings settings;
        private VmSession vm;
        private bool installing;
        private bool busy;
        private bool closing;
        private bool refreshingDisks;
        private CancellationTokenSource operationCancellation;
        private string lastLog;
        private string lastDiskDirectory;

        private readonly TabControl tabs = new TabControl { Dock = DockStyle.Fill, Padding = new Point(22, 8) };
        private readonly Label vmStatus = TextLabel("", 10, FontStyle.Bold);
        private readonly Label status = TextLabel("Выберите установочный ISO или укажите пути к инструментам в настройках.", 10);
        private readonly Button cancel = ActionButton("Отмена", false, 108);
        private readonly ComboBox systems = new ComboBox { Dock = DockStyle.Top, DropDownStyle = ComboBoxStyle.DropDownList };
        private readonly NumericUpDown diskSize = Number(AppSettings.MinDiskSize, AppSettings.MaxDiskSize, 32);
        private readonly Button installSystem = ActionButton("Установить из ISO…", true, 203);
        private readonly Button createDisk = ActionButton("Создать пустой диск…", false, 203);
        private readonly Button startVm = ActionButton("Запустить Android", true, 184);
        private readonly Button stopVm = ActionButton("Выключить", false, 115);
        private readonly Label diskHint = TextLabel("", 10);
        private readonly TextBox diskFile = new TextBox
        {
            Dock = DockStyle.Fill, ReadOnly = true, AccessibleName = "Путь к файлу виртуального диска"
        };
        private readonly ListBox packages = new ListBox { Dock = DockStyle.Fill, IntegralHeight = false, BorderStyle = BorderStyle.FixedSingle };
        private readonly Button installApk = ActionButton("Установить и открыть APK…", true, 240);
        private readonly Button openApk = ActionButton("Открыть выбранное", false, 184);
        private readonly Button refreshApps = ActionButton("Обновить список", false, 162);
        private readonly TextBox qemu = new TextBox { Dock = DockStyle.Fill };
        private readonly TextBox adb = new TextBox { Dock = DockStyle.Fill };
        private readonly NumericUpDown memory;
        private readonly NumericUpDown cores;
        private readonly CheckBox software = new CheckBox { Text = "Всегда использовать программную эмуляцию (TCG)", AutoSize = true };
        private readonly Button saveSettings = ActionButton("Сохранить настройки", true, 230);
        private readonly Button probe = ActionButton("Проверить систему", false, 174);
        private readonly Button enableWhpx = ActionButton("Включить Windows Hypervisor Platform…", true, 325);
        private readonly Button armStatus = ActionButton("Проверить ARM", false, 150);
        private readonly Button showLogs = ActionButton("Журналы", false, 108);
        private readonly TextBox diagnostics = new TextBox
        {
            Dock = DockStyle.Fill, Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical,
            BorderStyle = BorderStyle.FixedSingle, BackColor = Color.White
        };

        public MainForm(SettingsStore store, AppSettings settings)
        {
            this.store = store;
            this.settings = settings;
            memory = Number(AppSettings.MinMemory, AppSettings.MaxMemory, settings.MemoryMb, 512);
            cores = Number(1, AppSettings.MaxCores, settings.Cores);
            software.Checked = settings.ForceSoftware;
            qemu.Text = settings.QemuPath ?? "";
            adb.Text = settings.AdbPath ?? "";
            Text = "android-emu — Android для Windows";
            Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
            Font = new Font("Segoe UI", 10);
            AutoScaleMode = AutoScaleMode.Dpi;
            MinimumSize = new Size(935, 646);
            ClientSize = new Size(1010, 665);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Background;
            ForeColor = Ink;

            var root = Column(Padding.Empty);
            Row(root, BuildHeader(), 88);
            var inset = new Panel { Dock = DockStyle.Fill, Padding = new Padding(18, 14, 18, 0) };
            inset.Controls.Add(tabs);
            Row(root, inset, fill: true);
            Row(root, BuildFooter(), 56);
            Controls.Add(root);

            BuildAndroidPage();
            BuildAppsPage();
            BuildSettingsPage();
            BuildDiagnosticsPage();
            cancel.Click += (_, __) => CancelOperation();
            FormClosing += BeforeClose;
            RefreshDisks();
            UpdateControls();
            RefreshDiagnostics();
        }

        private Control BuildHeader()
        {
            var header = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Padding = new Padding(24, 7, 24, 7), BackColor = Dark };
            header.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            header.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 242));
            var title = Column(Padding.Empty);
            title.BackColor = Dark;
            var name = TextLabel("android-emu", 23, FontStyle.Bold);
            name.ForeColor = Color.White;
            var subtitle = TextLabel("ANDROID ДЛЯ WINDOWS  /  QEMU  /  v" + Program.Version, 9, FontStyle.Bold);
            subtitle.ForeColor = Color.FromArgb(178, 199, 214);
            Row(title, name, 44);
            Row(title, subtitle, 23);
            vmStatus.ForeColor = Mint;
            vmStatus.TextAlign = ContentAlignment.MiddleRight;
            header.Controls.Add(title, 0, 0);
            header.Controls.Add(vmStatus, 1, 0);
            return header;
        }

        private Control BuildFooter()
        {
            var footer = new TableLayoutPanel { ColumnCount = 2, Dock = DockStyle.Fill, Padding = new Padding(22, 6, 14, 6), BackColor = Background };
            footer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            footer.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 120));
            status.ForeColor = Muted;
            footer.Controls.Add(status, 0, 0);
            footer.Controls.Add(cancel, 1, 0);
            return footer;
        }

        private void BuildAndroidPage()
        {
            var page = Page("Android", "01 / СИСТЕМА", "Виртуальные системы",
                "Установите Android с загрузочного ISO для ПК. Изменения приложений и системы сохраняются на виртуальном диске.");
            var columns = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, Padding = new Padding(0, 5, 0, 0) };
            columns.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 48));
            columns.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 52));
            var installed = Card();
            Row(installed, TextLabel("Установленный Android", 14, FontStyle.Bold), 37);
            Row(installed, TextLabel("Активная система", 10), 24);
            Row(installed, systems, 35);
            Row(installed, Actions(startVm, stopVm), 44);
            Row(installed, TextLabel("Файл диска для QEMU", 10), 24);
            Row(installed, diskFile, 32);
            diskHint.ForeColor = Muted;
            diskHint.TextAlign = ContentAlignment.TopLeft;
            Row(installed, diskHint, 52);
            var setup = Card();
            Row(setup, TextLabel("Новый виртуальный диск", 14, FontStyle.Bold), 37);
            Row(setup, TextLabel("Выберите, где создать файл QCOW2. Физические диски и общие папки в QEMU не подключаются.", 10), 50);
            Row(setup, Actions(TextLabel("Размер диска, ГиБ", 10), diskSize), 37);
            Row(setup, Actions(installSystem), 41);
            Row(setup, Actions(createDisk), 41);
            Row(setup, TextLabel("Для установки нужен ISO x86/x86_64 с Legacy BIOS.", 9), 37);
            columns.Controls.Add(installed, 0, 0);
            columns.Controls.Add(setup, 1, 0);
            Row(page, columns, fill: true);

            systems.SelectedIndexChanged += (_, __) => SelectSystem();
            installSystem.Click += async (_, __) => await PickIsoAsync();
            createDisk.Click += async (_, __) => await CreateEmptyDiskAsync();
            startVm.Click += async (_, __) => await RunAsync("Запускаем Android…", true, async token => { await EnsureVmAsync(token); });
            stopVm.Click += async (_, __) => await RunAsync("Выключаем Android…", false, async token => { await StopAndroidAsync(); });
        }

        private void BuildAppsPage()
        {
            var page = Page("Приложения", "02 / APK", "Приложения Android",
                "Устанавливайте одиночные APK. Для работы с приложениями в Android должна быть включена сетевая отладка ADB.");
            Row(page, Actions(installApk, openApk, refreshApps), 46);
            Row(page, packages, fill: true);
            Row(page, TextLabel("ARM32 и ARM64 проверяются отдельно по ABI запущенного Android. Подробности — в «Диагностике».", 9), 31);
            packages.SelectedIndexChanged += (_, __) => UpdateControls();
            packages.DoubleClick += async (_, __) => await OpenSelectedAsync();
            openApk.Click += async (_, __) => await OpenSelectedAsync();
            refreshApps.Click += async (_, __) => await RunAsync("Подключаемся к Android…", true, async token =>
            {
                var client = await GetAdbAsync(token);
                await RefreshAppsAsync(client, token);
                status.Text = "Список приложений обновлён.";
            });
            installApk.Click += async (_, __) => await PickApkAsync();
        }

        private void BuildSettingsPage()
        {
            var page = Page("Настройки", "03 / КОНФИГУРАЦИЯ", "Настройки эмулятора",
                "Оставьте пути пустыми для автоопределения. Параметры RAM, CPU и ускорения применяются при следующем запуске Android.");
            var panel = Card();
            var inputs = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 3 };
            inputs.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 123));
            inputs.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            inputs.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 111));
            var browseQemu = ActionButton("Обзор…", false, 103);
            var browseAdb = ActionButton("Обзор…", false, 103);
            InputRow(inputs, "Движок QEMU", qemu, browseQemu, 0);
            InputRow(inputs, "Инструмент ADB", adb, browseAdb, 1);
            Row(panel, inputs, 81);
            Row(panel, Actions(TextLabel("RAM, МиБ", 10), memory, TextLabel("Ядра CPU", 10), cores), 44);
            Row(panel, software, 40);
            Row(panel, TextLabel("Авто: WHPX, если доступна. При ошибке старта QEMU повторяет попытку с TCG.", 10), 36);
            Row(panel, Actions(saveSettings), 44);
            Row(page, panel, fill: true);
            browseQemu.Click += (_, __) => BrowseExecutable(qemu, "QEMU x86-64|qemu-system-x86_64.exe;qemu-system-x86_64w.exe|Программы|*.exe");
            browseAdb.Click += (_, __) => BrowseExecutable(adb, "ADB|adb.exe|Программы|*.exe");
            saveSettings.Click += (_, __) => SaveSettings();
        }

        private void BuildDiagnosticsPage()
        {
            var page = Page("Диагностика", "04 / ВОЗМОЖНОСТИ", "Диагностика и ускорение",
                "WHPX ускоряет x86/x86_64. Поддержка ARM отдельно зависит от транслятора в гостевой системе.");
            Row(page, Actions(enableWhpx, probe, armStatus, showLogs), 48);
            Row(page, diagnostics, fill: true);
            probe.Click += (_, __) => RefreshDiagnostics();
            enableWhpx.Click += async (_, __) => await EnableWhpxAsync();
            armStatus.Click += async (_, __) => await RunAsync("Читаем ABI гостевой системы…", true, async token =>
            {
                var client = await GetAdbAsync(token, startIfOff: false);
                var info = await client.ReadAbiAsync(token);
                RefreshDiagnostics(info.Describe());
                status.Text = "Проверка ARM завершена.";
            });
            showLogs.Click += (_, __) => TryUi(() =>
            {
                Directory.CreateDirectory(store.LogsDirectory);
                using (Process.Start(new ProcessStartInfo(store.LogsDirectory) { UseShellExecute = true })) { }
            });
        }

        private TableLayoutPanel Page(string tab, string tag, string title, string caption)
        {
            var page = new TabPage(tab) { BackColor = Background, Padding = new Padding(6) };
            var column = Column(new Padding(18, 13, 18, 13));
            var marker = TextLabel(tag, 9, FontStyle.Bold);
            marker.ForeColor = Accent;
            Row(column, marker, 22);
            Row(column, TextLabel(title, 19, FontStyle.Bold), 40);
            var text = TextLabel(caption, 10);
            text.ForeColor = Muted;
            Row(column, text, 38);
            page.Controls.Add(column);
            tabs.TabPages.Add(page);
            return column;
        }

        private static TableLayoutPanel Column(Padding padding)
        {
            var table = new TableLayoutPanel { ColumnCount = 1, RowCount = 0, Padding = padding, Dock = DockStyle.Fill, Margin = Padding.Empty };
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            return table;
        }

        private static TableLayoutPanel Card()
        {
            var card = Column(new Padding(16));
            card.BackColor = Color.White;
            card.Margin = new Padding(0, 2, 12, 4);
            return card;
        }

        private static void Row(TableLayoutPanel table, Control control, int height = 0, bool fill = false)
        {
            int row = table.RowCount++;
            table.RowStyles.Add(fill ? new RowStyle(SizeType.Percent, 100) : new RowStyle(SizeType.Absolute, height));
            table.Controls.Add(control, 0, row);
        }

        private static void InputRow(TableLayoutPanel table, string label, Control input, Button browse, int row)
        {
            table.RowCount++;
            table.RowStyles.Add(new RowStyle(SizeType.Absolute, 40));
            table.Controls.Add(TextLabel(label, 10), 0, row);
            input.Margin = new Padding(4, 8, 8, 3);
            table.Controls.Add(input, 1, row);
            table.Controls.Add(browse, 2, row);
        }

        private static FlowLayoutPanel Actions(params Control[] controls)
        {
            var flow = new FlowLayoutPanel { Dock = DockStyle.Fill, WrapContents = false, Margin = Padding.Empty };
            foreach (var control in controls)
            {
                if (control is Label)
                {
                    control.AutoSize = true;
                    control.Margin = new Padding(1, 8, 8, 0);
                }
                if (control is NumericUpDown) control.Margin = new Padding(0, 4, 17, 0);
                flow.Controls.Add(control);
            }
            return flow;
        }

        private static Label TextLabel(string text, float size, FontStyle style = FontStyle.Regular) => new Label
        {
            Text = text, Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleLeft,
            Font = new Font("Segoe UI", size, style), AutoEllipsis = true, Margin = Padding.Empty
        };

        private static NumericUpDown Number(int min, int max, int value, int step = 1) => new NumericUpDown
        {
            Minimum = min, Maximum = max, Value = value, Increment = step, Width = 87, TextAlign = HorizontalAlignment.Right
        };

        private static Button ActionButton(string text, bool primary, int width)
        {
            var button = new Button { Text = text, Width = width, Height = 35, Margin = new Padding(0, 2, 7, 2), FlatStyle = FlatStyle.Flat };
            button.BackColor = primary ? Accent : Color.White;
            button.ForeColor = primary ? Color.White : Ink;
            button.FlatAppearance.BorderColor = primary ? Accent : Color.FromArgb(186, 201, 214);
            button.FlatAppearance.MouseOverBackColor = primary ? Color.FromArgb(13, 81, 70) : Color.FromArgb(231, 241, 244);
            return button;
        }

        private void UpdateControls()
        {
            var running = vm != null && !vm.Exit.IsCompleted;
            vmStatus.Text = running ? (installing ? "●  УСТАНОВКА ANDROID" : vm.HasWhpx ? "●  ANDROID  /  WHPX" : "●  ANDROID  /  TCG") : "○  ANDROID ВЫКЛЮЧЕН";
            systems.Enabled = !busy && !running && systems.Items.Count > 0;
            installSystem.Enabled = !busy && !running;
            createDisk.Enabled = !busy && !running;
            diskSize.Enabled = !busy && !running;
            startVm.Enabled = !busy && !running && settings.ActiveDisk != null;
            stopVm.Enabled = !busy && running;
            installApk.Enabled = !busy && !installing && settings.ActiveDisk != null;
            refreshApps.Enabled = !busy && running && !installing;
            openApk.Enabled = !busy && running && !installing && packages.SelectedIndex >= 0;
            saveSettings.Enabled = !busy;
            enableWhpx.Enabled = !busy && !Hypervisor.IsAvailable();
            armStatus.Enabled = !busy && running && !installing;
            probe.Enabled = !busy;
            cancel.Enabled = operationCancellation != null && !operationCancellation.IsCancellationRequested;
            var disk = settings.ActiveDisk;
            diskFile.Text = disk == null ? "" : store.DiskPath(disk);
            diskHint.Text = disk == null
                ? "Системы пока нет. Начните с установки из ISO."
                : "Скопируйте путь, если хотите указать диск в QEMU вручную. Не открывайте его в двух машинах одновременно.";
        }

        private void RefreshDisks()
        {
            refreshingDisks = true;
            try
            {
                systems.Items.Clear();
                foreach (var disk in settings.Disks) systems.Items.Add(disk);
                systems.SelectedItem = settings.ActiveDisk;
            }
            finally { refreshingDisks = false; }
        }

        private void SelectSystem()
        {
            if (refreshingDisks || busy || vm != null && !vm.Exit.IsCompleted || !(systems.SelectedItem is AndroidDisk disk)) return;
            TryUi(() =>
            {
                var next = settings.Copy();
                next.ActiveDiskId = disk.Id;
                Persist(next);
                status.Text = "Выбрана система «" + disk.Name + "».";
            });
            RefreshDisks();
            RefreshDiagnostics();
        }

        private void Persist(AppSettings next)
        {
            store.Save(next);
            settings = next;
        }

        private void SaveSettings()
        {
            TryUi(() =>
            {
                var next = settings.Copy();
                next.QemuPath = qemu.Text.Trim().Trim('"');
                next.AdbPath = adb.Text.Trim().Trim('"');
                if (next.QemuPath.Length != 0) next.QemuPath = ToolPaths.FindQemu(next);
                if (next.AdbPath.Length != 0) next.AdbPath = ToolPaths.FindAdb(next);
                next.MemoryMb = (int)memory.Value;
                next.Cores = (int)cores.Value;
                next.ForceSoftware = software.Checked;
                Persist(next);
                RefreshDiagnostics();
                status.Text = "Настройки сохранены.";
            });
        }

        private void BrowseExecutable(TextBox field, string filter)
        {
            using (var dialog = new OpenFileDialog { Filter = filter, CheckFileExists = true, RestoreDirectory = true })
                if (dialog.ShowDialog(this) == DialogResult.OK) field.Text = dialog.FileName;
        }

        private string PickNewDiskPath(string suggestedName)
        {
            var active = settings.ActiveDisk;
            var startingFolder = lastDiskDirectory ?? (active == null ? null : Path.GetDirectoryName(store.DiskPath(active)));
            using (var dialog = new SaveFileDialog
            {
                Title = "Где создать виртуальный диск QEMU?",
                Filter = "Виртуальный диск QEMU (*.qcow2)|*.qcow2",
                DefaultExt = "qcow2",
                AddExtension = true,
                CheckPathExists = true,
                RestoreDirectory = true,
                OverwritePrompt = false, // Existing files are rejected, never offered for replacement.
                InitialDirectory = Directory.Exists(startingFolder) ? startingFolder : Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments),
                FileName = suggestedName
            })
            {
                string destination = null;
                dialog.FileOk += (_, e) =>
                {
                    destination = null;
                    try
                    {
                        var path = VirtualDisk.CheckDestination(dialog.FileName);
                        if (settings.Disks.Any(d => string.Equals(store.DiskPath(d), path, StringComparison.OrdinalIgnoreCase)))
                            throw new IOException("Этот путь уже связан с системой. Выберите другое имя файла.");
                        destination = path;
                    }
                    catch (Exception error) when (error is IOException || error is InvalidDataException || error is ArgumentException ||
                        error is UnauthorizedAccessException || error is NotSupportedException)
                    {
                        e.Cancel = true;
                        MessageBox.Show(this, error.Message, "Виртуальный диск", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    }
                };
                if (dialog.ShowDialog(this) != DialogResult.OK || destination == null) return null;
                lastDiskDirectory = Path.GetDirectoryName(destination);
                return destination;
            }
        }

        private async Task CreateEmptyDiskAsync()
        {
            var path = PickNewDiskPath("android.qcow2");
            if (path == null) return;
            var size = (int)diskSize.Value;
            await RunAsync("Создаём пустой диск QCOW2…", true, async token =>
            {
                var qemuImg = ToolPaths.FindQemuImg(ToolPaths.FindQemu(settings));
                await VirtualDisk.CreateAsync(runner, qemuImg, path, size, token);
                status.Text = "Пустой диск создан. Его можно выбрать в QEMU.";
                if (MessageBox.Show(this, "Готов пустой диск на " + size + " ГиБ:\n\n" + path +
                    "\n\nВыберите этот файл как виртуальный жёсткий диск в QEMU. Скопировать путь?",
                    "Виртуальный диск создан", MessageBoxButtons.YesNo, MessageBoxIcon.Information) != DialogResult.Yes) return;
                try { Clipboard.SetText(path); }
                catch (System.Runtime.InteropServices.ExternalException)
                {
                    MessageBox.Show(this, "Буфер обмена занят. Диск уже создан:\n" + path, "Виртуальный диск",
                        MessageBoxButtons.OK, MessageBoxIcon.Information);
                }
            });
        }

        private async Task PickIsoAsync()
        {
            string iso;
            using (var dialog = new OpenFileDialog { Filter = "Установочный ISO (*.iso)|*.iso", CheckFileExists = true, RestoreDirectory = true })
            {
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                iso = dialog.FileName;
            }
            var stem = Path.GetFileNameWithoutExtension(iso);
            var path = PickNewDiskPath(string.IsNullOrWhiteSpace(stem) ? "android.qcow2" : stem + ".qcow2");
            if (path == null) return;
            var size = (int)diskSize.Value;
            if (MessageBox.Show(this, "Будет создан новый файл QCOW2 до " + size + " ГиБ:\n" + path + "\n\n" +
                "В окне QEMU выберите Installation и единственный виртуальный жёсткий диск этого размера. " +
                "Физические диски и общие папки в установщике не видны. Установите загрузчик для Legacy BIOS, затем закройте окно QEMU.\n\nПродолжить?",
                "Установка Android", MessageBoxButtons.OKCancel, MessageBoxIcon.Information) != DialogResult.OK) return;
            await RunAsync("Создаём виртуальный диск и запускаем установщик…", true, token => InstallIsoAsync(iso, path, size, token));
        }

        private async Task InstallIsoAsync(string iso, string path, int size, CancellationToken token)
        {
            IsoInstaller.CheckIso(iso);
            var qemuExe = ToolPaths.FindQemu(settings);
            var imgExe = ToolPaths.FindQemuImg(qemuExe);
            var disk = new AndroidDisk
            {
                Id = Guid.NewGuid().ToString("N"), Name = Path.GetFileNameWithoutExtension(path), SizeGiB = size, FilePath = path
            };
            if (settings.Disks.Any(d => d.Name == disk.Name)) disk.Name += " · " + disk.Id.Substring(0, 6);
            await VirtualDisk.CreateAsync(runner, imgExe, path, size, token);
            VmSession session = null;
            try
            {
                installing = true;
                session = await Qemu.StartAsync(qemuExe, settings, path, iso, store.LogsDirectory, token);
                vm = session;
                lastLog = session.LogPath;
                status.Text = "Установите Android на единственный виртуальный диск в QEMU, затем закройте окно QEMU.";
                UpdateControls();
                await WaitForExitAsync(session, token);
                await session.DrainOutputAsync();
                var code = await session.Exit;
                if (code != 0) throw new IOException("QEMU завершился с ошибкой " + code + ".\n" + session.LastError + "\nЖурнал: " + lastLog);
                if (MessageBox.Show(this, "Android установлен на виртуальный диск?\n\n" +
                    "«Да» — добавить диск в список и сделать активным. «Нет» — не добавлять; файл останется на месте.\n\n" + path,
                    "Завершение установки", MessageBoxButtons.YesNo, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2) == DialogResult.Yes)
                {
                    var next = settings.Copy();
                    next.Disks.Add(disk);
                    next.ActiveDiskId = disk.Id;
                    Persist(next);
                    RefreshDisks();
                    status.Text = "Диск добавлен. Нажмите «Запустить Android» для загрузки без ISO.";
                }
                else status.Text = "Диск не добавлен в список. Файл остался по выбранному пути.";
            }
            catch (Exception error) when (!(error is OperationCanceledException))
            {
                throw new IOException(error.Message + "\n\nСозданный файл диска оставлен по пути:\n" + path, error);
            }
            finally
            {
                if (session == null || await session.StopAsync(force: true))
                {
                    session?.Dispose();
                    vm = null;
                    installing = false;
                }
                else ObserveVm(session);
                RefreshDiagnostics();
            }
        }

        private static async Task WaitForExitAsync(VmSession session, CancellationToken cancellation)
        {
            var cancelled = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            using (cancellation.Register(() => cancelled.TrySetResult(true)))
            {
                await Task.WhenAny(session.Exit, cancelled.Task);
                cancellation.ThrowIfCancellationRequested();
            }
        }

        private async Task<VmSession> EnsureVmAsync(CancellationToken token)
        {
            if (vm != null && !vm.Exit.IsCompleted)
            {
                if (installing) throw new InvalidOperationException("Сначала завершите установку из ISO.");
                return vm;
            }
            var disk = settings.ActiveDisk ?? throw new InvalidOperationException("Сначала установите Android из ISO.");
            var session = await Qemu.StartAsync(ToolPaths.FindQemu(settings), settings, store.DiskPath(disk), null, store.LogsDirectory, token);
            vm = session;
            lastLog = session.LogPath;
            packages.Items.Clear();
            status.Text = session.HasWhpx ? "Android запущен с WHPX." : "Android запущен с TCG. Первая загрузка может занять несколько минут.";
            RefreshDiagnostics();
            ObserveVm(session);
            UpdateControls();
            return session;
        }

        private async void ObserveVm(VmSession session)
        {
            var exit = await session.Exit;
            await session.DrainOutputAsync();
            session.Dispose();
            if (!ReferenceEquals(vm, session) || IsDisposed) return;
            vm = null;
            installing = false;
            packages.Items.Clear();
            if (!busy) status.Text = exit == 0 ? "Android выключен." : "QEMU завершился с кодом " + exit + ". Откройте журналы в диагностике.";
            UpdateControls();
            RefreshDiagnostics();
        }

        private async Task<bool> StopAndroidAsync()
        {
            var session = vm;
            if (session == null || await session.StopAsync(force: false))
            {
                status.Text = "Android выключен.";
                return true;
            }
            if (MessageBox.Show(this, "Android не ответил на команду выключения.\n" +
                "Выключите его из меню Android или остановите QEMU принудительно (несохранённые данные могут быть потеряны).\n\nОстановить принудительно?",
                "Выключение Android", MessageBoxButtons.YesNo, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2) != DialogResult.Yes)
            {
                status.Text = "Android продолжает работать.";
                return false;
            }
            if (!await session.StopAsync(force: true)) throw new IOException("QEMU не удалось остановить.");
            status.Text = "QEMU остановлен.";
            return true;
        }

        private async Task<Adb> GetAdbAsync(CancellationToken token, bool startIfOff = true)
        {
            var adbExe = ToolPaths.FindAdb(settings);
            var session = vm;
            if (session == null || session.Exit.IsCompleted)
            {
                if (!startIfOff) throw new InvalidOperationException("Запустите Android для проверки ARM.");
                session = await EnsureVmAsync(token);
            }
            if (installing) throw new InvalidOperationException("ADB недоступен в установщике ISO.");
            var client = new Adb(adbExe, session.AdbPort, runner);
            status.Text = "Ждём Android и ADB. При запросе авторизации подтвердите подключение в госте.";
            using (var linked = CancellationTokenSource.CreateLinkedTokenSource(token))
            {
                var boot = client.WaitForBootAsync(TimeSpan.FromMinutes(4), linked.Token);
                if (await Task.WhenAny(boot, session.Exit) == session.Exit)
                {
                    linked.Cancel();
                    try { await boot; } catch (OperationCanceledException) { }
                    token.ThrowIfCancellationRequested();
                    throw new IOException("QEMU завершился до подключения ADB. Журнал: " + session.LogPath);
                }
                await boot;
            }
            return client;
        }

        private async Task PickApkAsync()
        {
            string path;
            using (var dialog = new OpenFileDialog { Filter = "Приложение Android (*.apk)|*.apk", CheckFileExists = true, RestoreDirectory = true })
            {
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                path = dialog.FileName;
            }
            await RunAsync("Проверяем APK…", true, async token =>
            {
                var apk = await Task.Run(() => ApkInfo.Read(path), token);
                var client = await GetAdbAsync(token);
                status.Text = "Устанавливаем «" + apk.Package + "»…";
                await client.InstallAsync(path, apk, token);
                var launched = await client.LaunchAsync(apk.Package, token);
                if (!packages.Items.Contains(apk.Package)) packages.Items.Add(apk.Package);
                packages.SelectedItem = apk.Package;
                status.Text = launched ? "Открыто: " + apk.Package : "APK установлен. У приложения нет экрана запуска.";
            });
        }

        private async Task OpenSelectedAsync()
        {
            if (!(packages.SelectedItem is string package)) return;
            await RunAsync("Открываем приложение…", true, async token =>
            {
                var client = await GetAdbAsync(token, startIfOff: false);
                status.Text = await client.LaunchAsync(package, token) ? "Открыто: " + package : "У приложения нет экрана запуска.";
            });
        }

        private async Task RefreshAppsAsync(Adb client, CancellationToken token)
        {
            var apps = await client.ListPackagesAsync(token);
            packages.Items.Clear();
            packages.Items.AddRange(apps);
        }

        private async Task EnableWhpxAsync()
        {
            if (Hypervisor.IsAvailable()) { RefreshDiagnostics(); return; }
            if (MessageBox.Show(this, "Включить компонент Windows Hypervisor Platform?\n\n" +
                "Windows запросит права администратора для DISM. Если потребуется перезагрузка, её можно сделать вручную позже.",
                "Аппаратное ускорение WHPX", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
            await RunAsync("Ожидаем подтверждение Windows и завершение DISM…", false, async _ =>
            {
                var result = await Hypervisor.EnableAsync();
                RefreshDiagnostics();
                status.Text = result == HypervisorEnableResult.Cancelled ? "Запрос прав администратора отменён."
                    : result == HypervisorEnableResult.RestartRequired ? "Компонент включён. Для WHPX перезагрузите Windows вручную."
                    : Hypervisor.IsAvailable() ? "WHPX доступна для новых запусков Android."
                    : "DISM завершился успешно. Если WHPX недоступна, перезагрузите Windows и проверьте BIOS/UEFI.";
                if (result != HypervisorEnableResult.Cancelled)
                    MessageBox.Show(this, status.Text, "Windows Hypervisor Platform", MessageBoxButtons.OK, MessageBoxIcon.Information);
            });
        }

        private void RefreshDiagnostics(string guest = null)
        {
            var result = "android-emu " + Program.Version + " · Windows x64\n\n" +
                "QEMU: " + ToolHint(() => ToolPaths.FindQemu(settings)) + "\n" +
                "ADB: " + ToolHint(() => ToolPaths.FindAdb(settings)) + "\n" +
                "WHPX: " + (Hypervisor.IsAvailable() ? "доступна Windows" : "недоступна (компонент Windows, перезагрузка или VT-x/AMD-V в BIOS/UEFI)") + "\n" +
                "Ускорение при запуске: " + (settings.ForceSoftware ? "TCG (выбрано в настройках)" : "авто (WHPX / TCG)") + "\n" +
                "Система: " + (settings.ActiveDisk?.Name ?? "не установлена") + "\n" +
                (settings.ActiveDisk == null ? "" : "Файл диска: " + store.DiskPath(settings.ActiveDisk) + "\n") +
                (vm == null || installing || vm.Exit.IsCompleted ? "" : "ADB-устройство: 127.0.0.1:" + vm.AdbPort + "\n") +
                (lastLog == null ? "" : "Последний журнал QEMU: " + lastLog + "\n") +
                "\n" + (guest == null ? "Чтобы узнать ABI, запустите Android и нажмите «Проверить ARM»." : guest) + "\n\n" + Adb.ArmHint;
            diagnostics.Text = result.Replace("\n", "\r\n");
            diagnostics.SelectionStart = 0;
            diagnostics.ScrollToCaret();
            UpdateControls();
        }

        private static string ToolHint(Func<string> find)
        {
            try { return find(); }
            catch (Exception e) when (e is IOException || e is ArgumentException || e is UnauthorizedAccessException || e is NotSupportedException)
            { return e.Message; }
        }

        private async Task RunAsync(string progress, bool cancellable, Func<CancellationToken, Task> operation)
        {
            if (busy) return;
            busy = true;
            operationCancellation = cancellable ? new CancellationTokenSource() : null;
            status.Text = progress;
            UpdateControls();
            try { await operation(operationCancellation?.Token ?? CancellationToken.None); }
            catch (OperationCanceledException) { status.Text = "Операция отменена."; }
            catch (Exception e) { ShowError(e); }
            finally
            {
                operationCancellation?.Dispose();
                operationCancellation = null;
                busy = false;
                UpdateControls();
            }
        }

        private void CancelOperation()
        {
            var source = operationCancellation;
            if (source == null) return;
            if (installing && MessageBox.Show(this, "Прервать установку? Созданный файл виртуального диска не будет удалён.", "Отмена установки",
                MessageBoxButtons.YesNo, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2) != DialogResult.Yes) return;
            if (!ReferenceEquals(source, operationCancellation)) return;
            source.Cancel();
            status.Text = "Отменяем операцию…";
            UpdateControls();
        }

        private void TryUi(Action action)
        {
            try { action(); }
            catch (Exception e) { ShowError(e); }
        }

        private void ShowError(Exception e)
        {
            status.Text = "Операция не завершилась. Подробности в сообщении об ошибке.";
            MessageBox.Show(this, e.Message, "android-emu", MessageBoxButtons.OK, MessageBoxIcon.Warning);
        }

        private async void BeforeClose(object sender, FormClosingEventArgs e)
        {
            if (closing || e.CloseReason == CloseReason.WindowsShutDown || e.CloseReason == CloseReason.TaskManagerClosing) return;
            if (!busy && (vm == null || vm.Exit.IsCompleted)) return;
            e.Cancel = true;
            if (busy)
            {
                status.Text = "Дождитесь завершения операции или нажмите «Отмена».";
                return;
            }
            if (MessageBox.Show(this, "Выключить Android и закрыть android-emu?", "Выход", MessageBoxButtons.YesNo,
                MessageBoxIcon.Question) != DialogResult.Yes) return;
            bool stopped = false;
            await RunAsync("Выключаем Android…", false, async _ => stopped = await StopAndroidAsync());
            if (stopped) { closing = true; Close(); }
        }
    }
}
