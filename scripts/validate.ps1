# End-to-end validation of the assignment flow against a running server.
param([string]$Base = "http://127.0.0.1:8080")
$ErrorActionPreference = "Stop"

function Api($Method, $Path, $Body = $null, $Token = $null) {
    $h = @{}; if ($Token) { $h.Authorization = "Bearer $Token" }
    $args = @{ Method = $Method; Uri = "$Base$Path"; Headers = $h; ContentType = "application/json" }
    if ($Body) { $args.Body = ($Body | ConvertTo-Json -Depth 5) }
    Invoke-RestMethod @args
}
function Login($Email, $Password) {
    $c = Api POST /auth/login @{ email = $Email; password = $Password }
    Write-Host "login challenge for ${Email}: $($c.login_challenge_id) (no JWT returned)"
    $mail = Api GET "/dev/email-logs/latest?to=$Email"
    Write-Host "  code from dev email log: $($mail.code)"
    (Api POST /auth/verify-2fa @{ login_challenge_id = $c.login_challenge_id; code = $mail.code }).access_token
}

Api POST /seed/users | Out-Null
$admin = Login "admin@example.com" "Admin@12345"

$ids = @()
foreach ($p in "high", "medium", "low", "medium", "low") {
    $ids += (Api POST /tasks @{ title = "Mission ($p)"; description = "Created by Admin"; priority = $p } $admin).id
}
Write-Host "created $($ids.Count) tasks"
Api POST /tasks/assign @{ task_ids = $ids[0..2]; assignee_email = "jamesbond@example.com" } $admin | Out-Null
Write-Host "assigned 3 tasks to James Bond"

$james = Login "jamesbond@example.com" "Bond@007007"
try { Api POST /tasks @{ title = "unauthorised" } $james | Out-Null; throw "expected 403" }
catch { Write-Host "James create task -> $([int]$_.Exception.Response.StatusCode)" }

$first = Api GET /tasks/view-my-tasks $null $james
Write-Host "view-my-tasks #1: $($first.summary.total_assigned_tasks) tasks, cache.hit=$($first.cache.hit)"
$second = Api GET /tasks/view-my-tasks $null $james
Write-Host "view-my-tasks #2: $($second.summary.total_assigned_tasks) tasks, cache.hit=$($second.cache.hit)"
$first | ConvertTo-Json -Depth 5
