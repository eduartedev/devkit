<?php
declare(strict_types=1);
header('X-Powered-By: DevKit');
$version = htmlspecialchars(PHP_VERSION, ENT_QUOTES, 'UTF-8');
?>
<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <title>DevKit</title>
  <style>
    :root { color-scheme: dark; }
    body { font-family: "Share Tech Mono", ui-monospace, monospace; background:#12151a; color:#e8e4dc; margin:48px; }
    a { color:#3ee0a0; }
    .ok { color:#3ee0a0; }
  </style>
</head>
<body>
  <h1>DevKit</h1>
  <p class="ok">PHP <?php echo $version; ?> en 127.0.0.1:8080</p>
  <p><a href="http://adminer.test:8080/">Adminer</a> (si el DNS/hosts de DevKit está activo)</p>
</body>
</html>
