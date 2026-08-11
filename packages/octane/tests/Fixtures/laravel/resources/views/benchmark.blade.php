<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>{{ $title }}</title></head>
<body>
<main>
    <h1>{{ $title }}</h1>
    <ul>
        @foreach ($items as $item)
            <li data-id="{{ $item }}">Item {{ $item }}</li>
        @endforeach
    </ul>
</main>
</body>
</html>
