<?php

declare(strict_types=1);

$database = dirname(__DIR__, 2).'/packages/octane/tests/Fixtures/laravel/storage/benchmark.sqlite';
$pdo = new PDO('sqlite:'.$database, options: [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
$pdo->exec('CREATE TABLE IF NOT EXISTS benchmark_items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)');
$pdo->beginTransaction();
$pdo->exec('DELETE FROM benchmark_items');
$insert = $pdo->prepare('INSERT INTO benchmark_items (name) VALUES (?)');
foreach (range(1, 100) as $id) {
    $insert->execute(["item-{$id}"]);
}
$pdo->commit();
