# Minimal local web server for the prebuilt WEPANET web app (web-app/).
# Uses only .NET's HttpListener, so it works on any Windows PC without
# installing anything. Browsers refuse to run WebAssembly and Web Workers
# from file:// pages, which is why a local server is needed at all.
param([string]$Root = (Join-Path $PSScriptRoot "..\web-app"))

$Root = (Resolve-Path $Root).Path
if (-not (Test-Path (Join-Path $Root "index.html"))) {
    Write-Host "web-app\index.html not found. Did you download the whole repository?" -ForegroundColor Red
    Read-Host "Press Enter to close"
    exit 1
}

$docs = @{}
foreach ($name in "Manual.html", "Tutorial.html") {
    $docs[$name] = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\docs\$name"))
}

$types = @{
    ".html" = "text/html; charset=utf-8"; ".js" = "text/javascript"; ".wasm" = "application/wasm"
    ".css" = "text/css"; ".svg" = "image/svg+xml"; ".png" = "image/png"; ".json" = "application/json"
    ".inp" = "text/plain"; ".ico" = "image/x-icon"
}

# Pick the first free port from 8780 up.
$listener = $null
foreach ($port in 8780..8799) {
    $l = New-Object System.Net.HttpListener
    $l.Prefixes.Add("http://localhost:$port/")
    try { $l.Start(); $listener = $l; break } catch { $l.Close() }
}
if (-not $listener) {
    Write-Host "Could not start the local server (ports 8780-8799 are busy)." -ForegroundColor Red
    Read-Host "Press Enter to close"
    exit 1
}

$url = "http://localhost:$port/"
Write-Host ""
Write-Host "  WEPANET is running at $url" -ForegroundColor Green
Write-Host "  Your browser should open automatically."
Write-Host "  Keep this window open while you use WEPANET; close it to stop."
Write-Host ""
Start-Process $url

while ($listener.IsListening) {
    $ctx = $listener.GetContext()
    try {
        $rel = [Uri]::UnescapeDataString($ctx.Request.Url.AbsolutePath.TrimStart('/'))
        if ($rel -eq "") { $rel = "index.html" }
        $file = [IO.Path]::GetFullPath((Join-Path $Root $rel))
        # Help's Manual and Tutorial are kept once, in docs/
        if (-not (Test-Path $file -PathType Leaf) -and $docs.ContainsKey($rel)) { $file = $docs[$rel] }
        if (($file.StartsWith($Root) -or $docs.ContainsValue($file)) -and (Test-Path $file -PathType Leaf)) {
            $bytes = [IO.File]::ReadAllBytes($file)
            $ext = [IO.Path]::GetExtension($file).ToLower()
            $ctx.Response.ContentType = if ($types.ContainsKey($ext)) { $types[$ext] } else { "application/octet-stream" }
            $ctx.Response.Headers.Add("Cache-Control", "no-cache")
            $ctx.Response.ContentLength64 = $bytes.Length
            $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else {
            $ctx.Response.StatusCode = 404
        }
    } catch {
        $ctx.Response.StatusCode = 500
    } finally {
        $ctx.Response.Close()
    }
}
