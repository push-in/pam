@php($status = \Nimbus\Session\Session::current()?->get('status'))
@if (is_string($status))
    <p class="status" role="status">{{ $status }}</p>
@endif
