# PAM Mobile

Build native Android applications with PHP components that stay alive for the
whole application session. PAM Mobile does not render HTML, start a WebView, or
ship a JavaScript runtime. Both authoring styles become the same native tree:

```text
PHP component
├── tags in a .pam.php file
│   └── <Screen><Column>...</Column></Screen>
└── typed PHP tree
    └── Screen::make(Column::make(...))
          ↓
Renderable → Element tree → Rust diff → Android views
```

Use tags for concise, visual composition. Use typed classes for the lowest-level
API or dynamically built trees. Both styles can coexist in one application.

## Create and run an application

Choose one preset:

```bash
# PAM Native primitives only
pam init my-mobile-app --template mobile

# PAM Native plus the official PAM UI component library
pam init my-mobile-app --template mobile-ui
```

Connect an Android device with USB debugging enabled, then run:

```bash
cd my-mobile-app
adb devices
pam mobile doctor .
pam mobile dev .
```

`pam mobile dev` builds the debug application, installs it through ADB, starts
it, and opens the development session. The generated Composer scripts provide
shortcuts for the common commands:

```bash
composer mobile:doctor
composer mobile:dev
composer mobile:build
composer mobile:benchmark
composer mobile:profile
```

## Project anatomy

```text
my-mobile-app/
├── index.php                 # application bootstrap
├── pam-native.json           # Android identity and native capabilities
├── composer.json             # dependencies and PSR-4 autoloading
├── composer.lock             # exact dependency versions
├── src/
│   ├── Screens/
│   │   ├── Home.pam.php
│   │   └── Details.pam.php
│   └── Components/
│       └── MetricCard.pam.php
├── vendor/                   # Composer packages
└── .pam-native/              # generated build/cache files
```

Commit `composer.lock` and `pam-native.json`. Treat `.pam-native/` as generated
output: do not put application source code there or edit it by hand.

The bootstrap registers every `*.pam.php` component below `src` and starts the
root screen:

```php
<?php

declare(strict_types=1);

use App\Screens\Home;
use Pam\MobileUi\Enum\ThemeMode;
use Pam\MobileUi\MobileUi;
use Pam\Native\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');
MobileUi::mode(ThemeMode::System);
App::run(App::make(Home::class));
```

For a core-only project, omit the two `Pam\MobileUi` imports and the
`MobileUi::mode(...)` call.

## Generate screens and components

PAM creates the class, namespace, template, and directories without
overwriting existing files:

```bash
pam mobile make:screen Home .
pam mobile make:screen Details .
pam mobile make:component MetricCard .
```

```text
src/
├── Screens/
│   ├── Home.pam.php
│   └── Details.pam.php
└── Components/
    └── MetricCard.pam.php
```

To make a generated screen the entry screen, change the class passed to
`App::make(...)` in `index.php`.

## A screen with tags

A `.pam.php` file contains one PHP component class followed by one
`<template>`. State and behavior stay in PHP; tags describe the native layout.

```php
<?php

declare(strict_types=1);

namespace App\Screens;

use Pam\Native\Attributes\State;
use Pam\Native\Component;

final class Home extends Component
{
    #[State]
    public int $count = 0;

    public function increment(): void
    {
        $this->count++;
    }
}
?>

<template>
    <Screen>
        <SafeAreaView class="flex-1 ui-surface">
            <Column class="flex-1 gap-6 p-6">
                <Column class="gap-2">
                    <Heading size="2xl">My first screen</Heading>
                    <Text class="text-muted-foreground">
                        Native UI controlled by persistent PHP.
                    </Text>
                </Column>

                <Card class="gap-4 p-5">
                    <Text>Count: {{ $count }}</Text>

                    <Button size="lg" on:press="increment">
                        <ButtonText>Increment</ButtonText>
                    </Button>
                </Card>
            </Column>
        </SafeAreaView>
    </Screen>
</template>
```

Generated core components use `@press="increment"`. PAM UI also accepts the
explicit native event form `on:press="increment"`. Pick one convention per
project and keep event handlers as public PHP methods.

### Tag syntax

| Syntax | Purpose |
| --- | --- |
| `<Text>...</Text>` | Creates a native component by tag name |
| `class="gap-4 p-6"` | Applies utility classes |
| `fontSize="28"` | Passes a literal property |
| `:elevated="$featured"` | Binds a property to a PHP expression |
| `{{ $count }}` | Interpolates a value as text |
| `@press="save"` | Calls a public method for a native event |
| `bind:value="$email"` | Synchronizes an input with component state |
| `v-if="$ready"` | Renders a conditional branch |
| `v-for="$item in $items"` | Repeats an element |
| `v-for="$number in $count"` | Repeats one-based from `1` through an integer |
| `key="..."` | Preserves identity when repeated children move |

Template expressions support component properties, loop locals, arrays,
comparisons, boolean operators, ternaries, and public component methods. Put
validation, transformations, I/O, and business rules in PHP methods.

## The same screen as a typed tree

Tags are optional. This component produces the same native element tree:

```php
<?php

declare(strict_types=1);

namespace App\Screens;

use Pam\Native\Component;
use Pam\Native\Element;
use Pam\Native\Style;
use Pam\Native\UI\Button;
use Pam\Native\UI\Column;
use Pam\Native\UI\SafeAreaView;
use Pam\Native\UI\Screen;
use Pam\Native\UI\Text;

final class Counter extends Component
{
    private int $count = 0;

    public function render(): Element
    {
        return Screen::make(
            SafeAreaView::make(
                Column::make(
                    Text::make('My first screen')
                        ->style(new Style(fontSize: 28)),
                    Text::make("Count: {$this->count}"),
                    Button::make('Increment')
                        ->onPress($this->increment(...)),
                )->style(new Style(
                    flexGrow: 1,
                    padding: 24,
                    gap: 16,
                )),
            ),
        );
    }

    public function increment(): void
    {
        $this->count++;
    }
}
```

Use `Style` values in typed trees and utility classes in templates. Put `gap`
on the parent `Column` or `Row` instead of repeating margins on every child.

## Compose a screen as a tree

Keep screens readable by extracting repeated sections:

```text
Home
└── Screen
    └── SafeAreaView
        └── Column
            ├── PageHeader
            │   ├── Heading
            │   └── Text
            ├── MetricCard
            │   ├── Heading
            │   └── named slot: action
            └── Button
```

Create `src/Components/MetricCard.pam.php`:

```php
<?php

declare(strict_types=1);

namespace App\Components;

use Pam\Native\Component;

final class MetricCard extends Component
{
    public function __construct(
        public string $title,
        public string $value,
        public ?string $hint = null,
        public bool $elevated = false,
    ) {
    }
}
?>

<template>
    <Card :class="['gap-3 p-5', 'elevation-2' => $elevated]">
        <Row class="items-center justify-between">
            <Column class="gap-1">
                <Text class="text-muted-foreground">{{ $title }}</Text>
                <Heading size="xl">{{ $value }}</Heading>
            </Column>

            <Slot name="action" />
        </Row>

        <Text v-if="$hint" class="text-muted-foreground">{{ $hint }}</Text>
        <Slot />
    </Card>
</template>
```

Constructor-promoted public properties are component props. A parameter
without a default is required; a PHP default makes it optional. Use the
discovered class by its short name:

```xml
<MetricCard
    title="Revenue"
    value="R$ 12.480"
    hint="Up 18% this month"
    :elevated="true"
>
    <template #action>
        <Badge variant="success">
            <BadgeText>Live</BadgeText>
        </Badge>
    </template>

    <Text>Updated a few seconds ago</Text>
</MetricCard>
```

## State, conditions, and repetition

Normal PHP properties are the source of truth. `#[State]` marks local reactive
state for tooling and makes the intent explicit:

```php
#[State]
public bool $loading = false;

#[State]
public array $tasks = [
    ['id' => 1, 'title' => 'Design the screen', 'done' => true],
    ['id' => 2, 'title' => 'Test on Android', 'done' => false],
];
```

```xml
<ActivityIndicator v-if="$loading" />

<Column v-else class="gap-3">
    <Text v-if="$tasks === []">No tasks yet.</Text>

    <Row
        v-for="$task in $tasks"
        :key="$task['id']"
        class="items-center gap-3"
    >
        <Text>{{ $task['done'] ? '✓' : '○' }}</Text>
        <Text>{{ $task['title'] }}</Text>
    </Row>
</Column>
```

Always provide a stable `key` for reordered or editable collections. Use
`v-for` for short groups. For large datasets, use the native
`VirtualizedList`/`VirtualGrid` primitives so off-screen component trees are
recycled. An integer source is a direct repetition count:

```xml
<Skeleton v-for="$number in $placeholderCount" :key="$number" />
```

`$number` is `1`, `2`, `3`, and so on. Zero or a negative integer renders
nothing.

## Forms and two-way binding

Keep labels, controls, helper text, and errors in one vertical group:

```php
#[State]
public string $email = '';

#[State]
public bool $acceptedTerms = false;

#[State]
public ?string $error = null;

public function submit(): void
{
    $this->error = filter_var($this->email, FILTER_VALIDATE_EMAIL)
        ? null
        : 'Enter a valid email.';
}
```

```xml
<Card class="gap-5 p-5">
    <Column class="gap-2">
        <Text>Email</Text>
        <Input>
            <InputField
                bind:value="$email"
                keyboardType="email-address"
                placeholder="you@example.com"
                accessibilityLabel="Email"
            />
        </Input>
        <Text v-if="$error" class="text-error">{{ $error }}</Text>
    </Column>

    <Row class="items-center gap-3">
        <Checkbox bind:checked="$acceptedTerms">
            <CheckboxIndicator>
                <CheckboxIcon />
            </CheckboxIndicator>
        </Checkbox>
        <Text>I accept the terms</Text>
    </Row>

    <Button on:press="submit">
        <ButtonText>Continue</ButtonText>
    </Button>
</Card>
```

Use `bind:value` for text and `bind:checked` for boolean controls. Keep labels
visible even when an input has a placeholder. Give an interactive control an
accessibility label when its visible text does not describe the action.

## Navigation between screens

`Navigator` owns a named route stack. Route handlers are closures that return
a component or element tree:

```php
<?php

declare(strict_types=1);

use App\Screens\Details;
use App\Screens\Home;
use Pam\Native\App;
use Pam\Native\Navigation\Navigator;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');

$home = new Home();
$details = new Details();
$navigator = new Navigator(
    initialRoute: 'home',
    routes: [
        'home' => static fn () => $home,
        'details' => static fn () => $details,
    ],
    persistenceKey: 'main',
);

$home->navigator = $navigator;
$details->navigator = $navigator;

App::onBack(static function () use ($navigator): void {
    $navigator->pop();
});
App::run($navigator);
```

Declare the navigator on each screen and expose small action methods:

```php
use Pam\Native\Navigation\Navigator;

public Navigator $navigator;

public function openDetails(): void
{
    $this->navigator->push('details');
}

public function goBack(): void
{
    $this->navigator->pop();
}
```

```xml
<Button on:press="openDetails">
    <ButtonText>Open details</ButtonText>
</Button>
```

Android back calls `pop()`. At the root route, `pop()` returns `false` and
keeps the first screen mounted. Route names must be registered before use.

## Layout rules that prevent broken screens

- Start a full screen with `Screen` and `SafeAreaView`.
- Use `Column` for vertical flow and `Row` for horizontal groups.
- Put spacing between siblings on the parent with `gap-*`.
- Add `flex-1` only where a container should consume remaining space.
- Use `items-center` for cross-axis alignment.
- Avoid fixed widths for primary content; prefer `w-full` and `max-w-*`.
- Use horizontal scrolling only for an intentional carousel.
- Keep touch targets comfortably large; generated buttons are 52 pixels high.
- Test the smallest supported device, font scaling, software keyboard,
  light/dark theme, and long translated text.

For forms covered by the keyboard, compose `KeyboardAvoidingView` with a
scrolling container. For long data sets, prefer `FlatList`,
`VirtualizedList`, `VirtualGrid`, or `SectionList` over a large `ScrollView`.

`Grid` accepts arbitrary PAM component trees and defaults to 12 columns.
Children can span, offset and reorder at the mobile-first `sm`, `md`, `lg` and
`xl` breakpoints while keeping independent row/column gutters:

```xml
<Grid columns="12" gutterX="16" gutterY="16">
    <Card span="12" spanMd="8">Main</Card>
    <Card span="12" spanMd="4">Aside</Card>
</Grid>
```

For large image/component collections, `VirtualGrid::make(2, ...$cells)` uses
the native RecyclerView window rather than mounting the entire data set.

## Core native tags

| Area | Tags/classes |
| --- | --- |
| Layout | `Screen`, `View`, `Column`, `Row`, `Grid`, `SafeAreaView`, `Spacer` |
| Content | `Text`, `Image`, `ImageBackground` |
| Input | `Input`, `TextInput`, `Button`, `Pressable`, `Toggle`, `Switch` |
| Scrolling | `ScrollView`, `FlatList`, `VirtualizedList`, `VirtualGrid`, `SectionList` |
| Presentation | `Modal`, `ActivityIndicator`, `StatusBar`, `KeyboardAvoidingView` |
| Android | `DrawerLayoutAndroid`, `TouchableNativeFeedback`, `InputAccessoryView` |

The `mobile-ui` preset adds PAM UI providers, cards, headings, badges, composed
inputs, checkboxes, radio groups, drawers, popovers, menus, tabs, and other
accessible controls. Prefer library components for product UI and core
primitives for custom layout or low-level integration.

## Lifecycle

Override only the hooks a component needs:

```php
public function boot(): void {}
public function mount(): void {}
public function rendered(): void {}
public function attached(): void {}
public function resumed(): void {}
public function updated(string $property): void {}
public function paused(): void {}
public function unmount(): void {}
```

`boot()` runs once per instance, `mount()` on its first render, `rendered()`
after each render pass, and `attached()` after the first native commit. Use
`resumed()` and `paused()` for app-state changes and `unmount()` to release
resources. Do not start subscriptions or timers inside `render()`.

## Native configuration

`pam-native.json` defines the Android application:

```json
{
  "$schema": "vendor/pushinbr/pam-native/resources/pam-native.schema.json",
  "version": 1,
  "applicationId": "com.example.myapp",
  "name": "My App",
  "entry": "index.php",
  "versionCode": 1,
  "versionName": "0.1.0",
  "android": {
    "minSdk": 26,
    "targetSdk": 36,
    "permissions": []
  },
  "modules": [],
  "views": []
}
```

Change `applicationId` before distribution. Increment `versionCode` for every
Android release and use `versionName` for the human-readable version.

Generate a custom Android view only when a core or PAM UI component cannot
express the platform feature:

```bash
pam mobile make:native-view CameraPreview .
```

## Build, diagnose, and profile

```bash
pam mobile doctor .             # validate toolchain and project
pam mobile dev .                # build, install, launch, develop
pam mobile build . --release    # create a release build
pam mobile benchmark .          # measure the application
pam mobile profile .            # generate a baseline profile
```

Before considering a screen finished:

1. Run `pam mobile doctor .`.
2. Open every interactive state on a real device.
3. Verify keyboard, back navigation, loading, empty, error, and long-content
   states.
4. Inspect scrolling and animations for dropped frames.
5. Build the release variant, not only debug.

If a freshly installed CLI differs from this guide, compare `pam --version`
with the latest release and ensure `~/.local/bin` appears before older PAM
installations in `PATH`.

## Continue reading

- [Native runtime API](native-api.md)
- [Package compatibility](packages.md)
- [Production operations](production.md)
- [Repository architecture](architecture.md)
