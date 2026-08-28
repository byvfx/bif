# Turn a native crash (STATUS_ACCESS_VIOLATION and friends) into file:line,
# with no debugger and no SDK install.
#
# Windows Error Reporting already logs the faulting module and fault offset for
# every unhandled exception, and dbghelp.dll ships with Windows. Given the
# matching PDB on disk, that is the whole answer -- `cdb` / "Debugging Tools for
# Windows" are not needed. This resolved issue #28 (a dangling QGraphicsItem in
# the node graph) after two sessions of instrumenting the wrong subsystem.
#
#   .\scripts\symbolize-crash.ps1                  # newest bif_viewer crash
#   .\scripts\symbolize-crash.ps1 -Count 5         # newest 5
#   .\scripts\symbolize-crash.ps1 -Match myapp     # a different executable
#
# Works on Qt/USD DLLs too, not just our own binaries, as long as the vendor
# shipped PDBs (Qt 6 does, under the same bin/ directory).
#
# Two things to know:
#
#  * A crash must actually reach WER. A console `cargo run` produces an event;
#    a process launched detached via Start-Process was observed NOT to. If
#    nothing shows up, re-run the crash from a normal console.
#
#  * RVAs are only meaningful against the exact binary that faulted. Rebuild
#    after a crash and symbolizing yields a confident, completely wrong line
#    number -- this bit us once. The script compares the on-disk PE
#    TimeDateStamp against the one WER recorded and refuses on mismatch, so a
#    STALE BINARY warning means "go rebuild the old commit", not "ignore me".

param([int]$Count = 1, [string]$Match = 'bif_viewer')

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class Dbg {
    [DllImport("dbghelp.dll", SetLastError=true)]
    public static extern bool SymInitialize(IntPtr h, string path, bool invade);
    [DllImport("dbghelp.dll", SetLastError=true)]
    public static extern ulong SymLoadModuleEx(IntPtr h, IntPtr file, string img,
        string mod, ulong baseAddr, uint size, IntPtr data, uint flags);
    [DllImport("dbghelp.dll", SetLastError=true)]
    public static extern bool SymFromAddr(IntPtr h, ulong addr, out ulong disp, IntPtr sym);
    [DllImport("dbghelp.dll", SetLastError=true)]
    public static extern bool SymGetLineFromAddr64(IntPtr h, ulong addr, out uint disp, IntPtr line);
    [DllImport("dbghelp.dll")] public static extern uint SymSetOptions(uint opts);
    [DllImport("dbghelp.dll")] public static extern bool SymCleanup(IntPtr h);

    public static string Resolve(string image, ulong rva) {
        IntPtr h = new IntPtr(-1);
        // SYMOPT_UNDNAME | SYMOPT_LOAD_LINES | SYMOPT_LOAD_ANYTHING
        SymSetOptions(0x2 | 0x10 | 0x40);
        string dir = System.IO.Path.GetDirectoryName(image);
        if (!SymInitialize(h, dir, false)) return "SymInitialize failed: " + Marshal.GetLastWin32Error();

        ulong loaded = SymLoadModuleEx(h, IntPtr.Zero, image, null, 0x10000000UL, 0, IntPtr.Zero, 0);
        if (loaded == 0) { int e = Marshal.GetLastWin32Error(); SymCleanup(h); return "SymLoadModuleEx failed: " + e; }
        ulong addr = loaded + rva;
        string result;

        int MAXNAME = 2000;
        IntPtr buf = Marshal.AllocHGlobal(88 + MAXNAME);
        try {
            for (int i = 0; i < 88 + MAXNAME; i++) Marshal.WriteByte(buf, i, 0);
            Marshal.WriteInt32(buf, 0, 88);       // SizeOfStruct
            Marshal.WriteInt32(buf, 84, MAXNAME); // MaxNameLen
            ulong disp;
            if (SymFromAddr(h, addr, out disp, buf)) {
                string name = Marshal.PtrToStringAnsi(IntPtr.Add(buf, 88));
                result = (string.IsNullOrEmpty(name) ? "<unnamed>" : name) + " + 0x" + disp.ToString("x");
            } else {
                result = "<no symbol> (err " + Marshal.GetLastWin32Error() + ")";
            }
        } finally { Marshal.FreeHGlobal(buf); }

        IntPtr lbuf = Marshal.AllocHGlobal(40);
        try {
            for (int i = 0; i < 40; i++) Marshal.WriteByte(lbuf, i, 0);
            Marshal.WriteInt32(lbuf, 0, 32);
            uint ldisp;
            if (SymGetLineFromAddr64(h, addr, out ldisp, lbuf)) {
                int line = Marshal.ReadInt32(lbuf, 16);
                IntPtr fp = Marshal.ReadIntPtr(lbuf, 24);
                string file = fp == IntPtr.Zero ? "?" : Marshal.PtrToStringAnsi(fp);
                result += "\n        @ " + file + ":" + line;
            }
        } finally { Marshal.FreeHGlobal(lbuf); }

        SymCleanup(h);
        return result;
    }
}
'@ -ErrorAction Stop

$events = Get-WinEvent -FilterHashtable @{LogName='Application'; ProviderName='Application Error'} `
            -MaxEvents 100 -ErrorAction SilentlyContinue |
          Where-Object { $_.Message -match $Match } |
          Select-Object -First $Count

if (-not $events) { "No '$Match' crashes found in the Application log."; return }

foreach ($e in $events) {
    $lines = $e.Message -split "`n"
    $modPath = (($lines | Where-Object { $_ -match 'Faulting module path:' }) -split ':\s*', 2)[1].Trim()
    $rvaText = (($lines | Where-Object { $_ -match 'Fault offset:' })      -split ':\s*', 2)[1].Trim()
    $exCode  = (($lines | Where-Object { $_ -match 'Exception code:' })    -split ':\s*', 2)[1].Trim()
    $rva     = [Convert]::ToUInt64($rvaText, 16)

    # WER records the faulting module's PE TimeDateStamp. If the on-disk module
    # has since been rebuilt, its RVAs no longer mean anything and symbolizing
    # yields a confident, wrong answer — so verify before trusting the result.
    $wantTs = (($lines | Where-Object { $_ -match 'Faulting module name:' }) -split 'time stamp:\s*')[1]
    $wantTs = if ($wantTs) { [Convert]::ToUInt32($wantTs.Trim(), 16) } else { 0 }

    "===== $($e.TimeCreated)  |  exception $exCode ====="
    "  module: $modPath  +0x{0:x}" -f $rva
    if (-not (Test-Path $modPath)) {
        "  <module no longer on disk — cannot symbolize>"
    } else {
        $fs = [IO.File]::OpenRead($modPath); $br = New-Object IO.BinaryReader($fs)
        $fs.Position = 0x3C; $peOff = $br.ReadInt32(); $fs.Position = $peOff + 8
        $haveTs = $br.ReadUInt32(); $br.Close()
        if ($wantTs -ne 0 -and $haveTs -ne $wantTs) {
            "  !! STALE BINARY — on-disk 0x{0:x8} != crashed 0x{1:x8}" -f $haveTs, $wantTs
            "     Rebuilt since this crash; RVA is meaningless. Not symbolizing."
        } else {
            "  " + [Dbg]::Resolve($modPath, $rva)
        }
    }
    ""
}
