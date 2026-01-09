# Test script with USD DLLs loaded
. .\setup_usd_env.ps1

Write-Host "Running tests with USD environment..." -ForegroundColor Cyan

# Run core tests (needs USD DLLs)
Write-Host "`nTesting bif_core..." -ForegroundColor Yellow
cargo test --package bif_core --lib 2>&1 | Select-String -Pattern "(test result|FAILED|PASSED|running \d+ test)"

# Run renderer tests (no DLL deps)
Write-Host "`nTesting bif_renderer..." -ForegroundColor Yellow
cargo test --package bif_renderer --lib 2>&1 | Select-String -Pattern "(test result|FAILED|PASSED|running \d+ test)"

# Run math tests (no DLL deps)
Write-Host "`nTesting bif_math..." -ForegroundColor Yellow
cargo test --package bif_math --lib 2>&1 | Select-String -Pattern "(test result|FAILED|PASSED|running \d+ test)"

Write-Host "`nDone!" -ForegroundColor Green
