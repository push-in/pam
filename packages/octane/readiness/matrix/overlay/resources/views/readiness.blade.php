<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>{{ config('app.name') }} readiness</title></head>
<body>
<h1>Readiness</h1>
<ul>
@foreach ($items as $item)
    <li class="item-{{ $item }}">Item {{ $item }} of {{ count($items) }}</li>
@endforeach
</ul>
</body>
</html>
