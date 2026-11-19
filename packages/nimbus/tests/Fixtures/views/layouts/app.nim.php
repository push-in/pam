<html><head><title>@yield('title', 'Nimbus')</title>@stack('head')</head>
<body>
@section('sidebar')
<nav>layout-nav</nav>
@show
<main>@yield('content')</main>
</body></html>
