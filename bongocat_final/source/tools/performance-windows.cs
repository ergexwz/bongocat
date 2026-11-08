using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
public static class ProfileWindows {
    private delegate bool Callback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Callback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int capacity);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags);
    [DllImport("kernel32.dll")] private static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr process);
    [DllImport("kernel32.dll")] private static extern bool GetProcessTimes(IntPtr process, out long created, out long exited, out long kernel, out long user);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] private static extern bool QueryFullProcessImageName(IntPtr process, uint flags, System.Text.StringBuilder path, ref int capacity);
    public static bool OwnsProcess(uint pid, long expected, string root) {
        string prefix=Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar,Path.AltDirectorySeparatorChar)+Path.DirectorySeparatorChar;
        IntPtr process=OpenProcess(0x1000,false,pid);
        if(process==IntPtr.Zero) return false;
        try {
            long created,exited,kernel,user;
            if(!GetProcessTimes(process,out created,out exited,out kernel,out user) || Math.Abs(created-expected)>10000) return false;
            var path=new System.Text.StringBuilder(32768);int capacity=path.Capacity;
            return QueryFullProcessImageName(process,0,path,ref capacity) && path.ToString().StartsWith(prefix,StringComparison.OrdinalIgnoreCase);
        } finally { CloseHandle(process); }
    }
    public static int Close(Dictionary<int,long> owned, string root) {
        int count=0;
        EnumWindows((window,data)=>{
            uint pid;
            GetWindowThreadProcessId(window,out pid);
            var name = new System.Text.StringBuilder(256);
            GetClassName(window, name, name.Capacity);
            long expected;
            if (!owned.TryGetValue((int)pid,out expected) || !IsWindowVisible(window) || name.ToString()!="Tauri Window") return true;
            if(OwnsProcess(pid,expected,root) && PostMessage(window,0x10,IntPtr.Zero,IntPtr.Zero)) count++;
            return true;
        },IntPtr.Zero);
        return count;
    }
}
