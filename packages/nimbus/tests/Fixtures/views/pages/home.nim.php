@extends('layouts.app')
@section('title', 'Home & co')
@push('head')
<meta name="page" content="home">
@endpush
@section('sidebar')
@parent
<p>child-sidebar</p>
@endsection
@section('content')
<h1>Hello, {{ $name }}</h1>
@include('partials.item', ['item' => 'included'])
@endsection
