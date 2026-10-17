# Generates icon.png (256 px), 32x32.png, 128x128.png and icon.ico for the Tauri app.
# Run from this folder:  powershell -ExecutionPolicy Bypass -File make_icon.ps1
Add-Type -AssemblyName System.Drawing

function New-G13Icon([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.TextRenderingHint = 'AntiAliasGridFit'
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $size / 256.0
    # hexagon of the logo top left (.brand-hex: 24-unit box, stroke 2.2), in blue #1E56E4
    $pts = @(
        (New-Object System.Drawing.PointF (128 * $s), (16 * $s)),
        (New-Object System.Drawing.PointF (229.3 * $s), (74.7 * $s)),
        (New-Object System.Drawing.PointF (229.3 * $s), (181.3 * $s)),
        (New-Object System.Drawing.PointF (128 * $s), (240 * $s)),
        (New-Object System.Drawing.PointF (26.7 * $s), (181.3 * $s)),
        (New-Object System.Drawing.PointF (26.7 * $s), (74.7 * $s))
    )
    $bg = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 27, 31, 39))
    $g.FillPolygon($bg, $pts)
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 30, 86, 228)), ([Math]::Max(2, 23.5 * $s))
    $g.DrawPolygon($pen, $pts)
    $g.Dispose()
    return $bmp
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$pngs = @{}
foreach ($sz in 16, 32, 48, 128, 256) {
    $b = New-G13Icon $sz
    $ms = New-Object System.IO.MemoryStream
    $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngs[$sz] = $ms.ToArray()
    if ($sz -eq 32) { [IO.File]::WriteAllBytes("$here\32x32.png", $pngs[$sz]) }
    if ($sz -eq 128) { [IO.File]::WriteAllBytes("$here\128x128.png", $pngs[$sz]) }
    if ($sz -eq 256) { [IO.File]::WriteAllBytes("$here\icon.png", $pngs[$sz]) }
    $b.Dispose()
}

# ICO container with embedded PNG images
$sizes = 16, 32, 48, 128, 256
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
foreach ($sz in $sizes) {
    $dim = if ($sz -ge 256) { 0 } else { $sz }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$pngs[$sz].Length); $w.Write([UInt32]$offset)
    $offset += $pngs[$sz].Length
}
foreach ($sz in $sizes) { $w.Write($pngs[$sz]) }
$w.Flush()
[IO.File]::WriteAllBytes("$here\icon.ico", $out.ToArray())
"icons written to $here"
