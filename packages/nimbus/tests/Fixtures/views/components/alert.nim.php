@props(['type' => 'info', 'dismissible' => false])
<div {{ $attributes->merge(['class' => 'alert alert-' . $type, 'role' => 'alert']) }}>
@isset($title)<strong>{{ $title }}</strong>@endisset
{{ $slot }}
@if ($dismissible)<button>x</button>@endif
@isset($footer)<footer {{ $footer->attributes }}>{{ $footer }}</footer>@endisset
</div>
