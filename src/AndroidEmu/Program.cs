using System;
using System.Reflection;
using System.Threading;
using System.Windows.Forms;
using AndroidEmu.Core;

namespace AndroidEmu
{
    internal static class Program
    {
        public static string Version => typeof(Program).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>().InformationalVersion;

        [STAThread]
        private static int Main(string[] args)
        {
            if (args.Length == 1 && args[0] == "--version")
            {
                Console.WriteLine("android-emu " + Version);
                return 0;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            using (var singleInstance = new Mutex(true, @"Local\android-emu.WinForms", out var firstInstance))
            {
                if (!firstInstance)
                {
                    MessageBox.Show("android-emu уже запущен.", "android-emu", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return 0;
                }
                try
                {
                    var store = new SettingsStore(SettingsStore.DefaultRoot);
                    using (var window = new MainForm(store, store.Load())) Application.Run(window);
                    return 0;
                }
                catch (Exception e)
                {
                    MessageBox.Show(e.Message, "android-emu — ошибка запуска", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    return 1;
                }
                finally { singleInstance.ReleaseMutex(); }
            }
        }
    }
}
