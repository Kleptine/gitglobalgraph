using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.CompilerServices;
using Conditions;

interface ITempDir : IDisposable
{
    string Path
    {
        get;
    }
}

class TempDir : ITempDir
{
    public string Path
    {
        get;
        private set;
    }

    /// <summary>
    /// Create a temp directory named after your test in the %temp%\uTest\xxx directory
    /// which is deleted and all sub directories when the ITempDir object is disposed.
    /// </summary>
    /// <returns></returns>
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static ITempDir Create()
    {
        var stack = new StackTrace(1);
        var sf = stack.GetFrame(0);
        return new TempDir(sf.GetMethod().Name);
    }

    public TempDir(string dirName)
    {
        if (String.IsNullOrEmpty(dirName))
        {
            throw new ArgumentException("dirName");
        }
        Path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "uTests", dirName + "-" + Guid.NewGuid());
        Directory.CreateDirectory(Path);

        Condition.Requires(Directory.GetFileSystemEntries(Path)).IsEmpty();
    }

    public void Dispose()
    {

        if (Path.Length < 10)
        {
            throw new InvalidOperationException(String.Format("Directory name seems to be invalid. Do not delete recursively your hard disc.", Path));
        }

        // and then the directory
        DeleteReadOnlyDirectory(Path);
    }

    /// <summary>
    /// Recursively deletes a directory as well as any subdirectories and files. If the files are read-only, they are flagged as normal and then deleted.
    /// </summary>
    /// <param name="directory">The name of the directory to remove.</param>
    private static void DeleteReadOnlyDirectory(string directory)
    {
        foreach (var subdirectory in Directory.EnumerateDirectories(directory))
        {
            DeleteReadOnlyDirectory(subdirectory);
        }
        foreach (var fileName in Directory.EnumerateFiles(directory))
        {
            var fileInfo = new FileInfo(fileName);
            fileInfo.Attributes = FileAttributes.Normal;
            fileInfo.Delete();
        }
        Directory.Delete(directory);
    }
}