using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Conditions;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace GitLocks.Tests
{
    public static class GitCmd
    {
        /// <summary>
        /// Runs an arbitrary git command with the bundled portable git installation.
        /// </summary>
        public static string Run(string workingDirectory, string args)
        {
            string gitCommand = Path.Combine(System.IO.Directory.GetCurrentDirectory() , @"..\..\GitBinaries\cmd\git.exe");

            var startInfo = new ProcessStartInfo(gitCommand, args)
            {
                WorkingDirectory = workingDirectory,
                RedirectStandardOutput = true,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                UseShellExecute = false,
            };

            Process process = Process.Start(startInfo);
            Condition.Requires(process).IsNotNull("Couldn't run git command line process");
            Condition.Requires(process.WaitForExit(5000)).IsTrue("Git command line process timed out after 5 seconds");

            return process.StandardOutput.ReadToEnd();
        }
    }
}
