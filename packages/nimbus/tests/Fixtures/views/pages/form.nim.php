<form method="POST" action="/submit">@csrf @method('put')
@auth<p>signed in</p>@endauth
@guest<p>guest</p>@endguest
@can('publish')<p>can publish</p>@endcan
@if ($errors->any())<ul>@foreach ($errors->all() as $error)<li>{{ $error }}</li>@endforeach</ul>@endif
<input name="title" value="{{ old('title') }}">
@lang('messages.welcome', ['name' => 'Ana'])
</form>
