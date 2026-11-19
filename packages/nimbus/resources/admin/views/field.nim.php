@php
$name = $field->name;
$described = trim(($field->help !== '' ? $id . '-help ' : '') . ($error !== null ? $id . '-error' : ''));
$common = ['id' => $id, 'name' => $name, 'aria-invalid' => $error !== null ? 'true' : null, 'aria-describedby' => $described !== '' ? $described : null, 'required' => $field->required, 'readonly' => $field->readonly, 'disabled' => $field->readonly && in_array($field->type, ['boolean', 'select', 'belongsTo'], true)];
$inputType = match ($field->type) { 'number', 'decimal' => 'number', 'date' => 'date', 'datetime' => 'datetime-local', 'time' => 'time', 'email' => 'email', 'url' => 'url', 'password' => 'password', default => 'text' };
$attributes = new \Nimbus\View\AttributeBag(array_filter($common, static fn (mixed $value): bool => $value !== null && $value !== false));
@endphp
<div class="field field-{{ $field->type }} @if ($error !== null) has-error @endif">
@if ($field->type === 'boolean')
<input type="hidden" name="{{ $name }}" value="0">
<label class="check-label"><input type="checkbox" value="1" {{ $attributes }} @checked($value === '1')> {{ $field->label }}</label>
@else
<label for="{{ $id }}">{{ $field->label }}@if ($field->required)<span class="req" aria-hidden="true"> *</span>@endif</label>
@if ($field->type === 'textarea' || $field->type === 'json')
<textarea rows="{{ $field->type === 'json' ? 8 : 5 }}" {{ $attributes }}>{{ $value }}</textarea>
@elseif ($field->type === 'select' || $field->type === 'belongsTo')
<select {{ $attributes }}>
<option value="">{{ $t($field->required ? 'choose' : 'none') }}</option>
@foreach ($options as $optionValue => $optionLabel)
<option value="{{ $optionValue }}" @selected((string) $optionValue === $value)>{{ $optionLabel }}</option>
@endforeach
</select>
@else
<input type="{{ $inputType }}" value="{{ $field->type === 'password' ? '' : $value }}" @if ($field->maxLength !== null) maxlength="{{ $field->maxLength }}" @endif @if ($field->type === 'decimal') step="any" @endif @if ($field->type === 'number') step="1" inputmode="numeric" @endif {{ $attributes }}>
@endif
@endif
@if ($field->help !== '')<p class="help" id="{{ $id }}-help">{{ $field->help }}</p>@endif
@if ($error !== null)<p class="error" id="{{ $id }}-error">{{ $error }}</p>@endif
</div>
