# Nimbus application

The starter API application for [Nimbus](https://github.com/push-in/nimbus) on
the PAM runtime.

## Start here

This skeleton creates an application for the PAM Runtime; it does not include
the runtime itself. Install PAM first, then create the project through PAM:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

pam doctor
pam init my-app
cd my-app
pam composer install
mkdir -p storage && touch storage/database.sqlite
export DB_CONNECTION=sqlite DB_DATABASE=$PWD/storage/database.sqlite
pam composer migrate
pam composer dev
```

The API starts at `http://127.0.0.1:3000`.

The starter demonstrates the structured Nimbus path: a named
controller method orchestrates a service and returns a JSON Resource. Closures
remain available for small endpoints, but application rules should live in
services and persistence should live in repositories.

It also ships an executable Arcano ORM vertical slice:

```bash
curl -X POST http://127.0.0.1:3000/api/products \
  -H 'content-type: application/json' \
  -d '{"name":"Mechanical keyboard","priceInCents":34990}'
curl http://127.0.0.1:3000/api/products
```

`ProductController` stays thin, `StoreProductRequest` validates and hydrates a
typed DTO, `ProductService` owns the use case, `ProductRepository` isolates
Arcano persistence, and `ProductResource` owns the response contract. Product
statuses are sequential integer-backed enum values (`1` active, `2` archived).
Creation returns `201 Created`; resources may select any valid HTTP status while
retaining Nimbus' consistent `data` envelope.

Run the in-memory application test inside Pam's Embed SAPI:

```bash
pam composer test
```

`tests/ApplicationTest.php` extends `Nimbus\Testing\TestCase` with `RefreshDatabase`: every test
migrates an in-memory SQLite database and rolls back what it wrote. `ProductFactory` builds
products with Faker; `pam composer seed` runs `database/seeders/DatabaseSeeder.php`.

The generated application and this published skeleton share the same source
files:

```text
app/
├── Controllers/
├── Data/                       # typed DTOs
├── Models/Product.php          # #[Entity] mapped by Arcano
├── Providers/AppServiceProvider.php
├── Repositories/
├── Requests/
├── Resources/
└── Services/
database/{migrations,factories,seeders}/
tests/{ApplicationTest.php,bootstrap.php}
bootstrap/app.php               # builds and returns the App (no listen)
bin/nimbus                      # bin/nimbus tinker
routes.php                      # every route, in one file
index.php
```

`routes.php` receives the router as `$router`:

```php
$router->get('api/ping', [PingController::class, 'show']);

$router->controller(ProductController::class, function (RouteRegistrar $router): void {
    $router->get('api/products', 'index');
    $router->post('api/products', 'store');
});
```

`bootstrap/app.php` installs secure response headers and boots Arcano from
environment configuration (`DB_*`); `index.php` validates typed configuration
and listens. `bin/nimbus tinker` opens a PsySH shell with `$app`, `$container`,
`$arcano` and `$orm` over the same application
(`bin/nimbus tinker --execute='$orm->getRepository(App\Models\Product::class)->findAll()'`).
The entities in `app/Models` are compiled into `var/cache/arcano-schema.php`;
the cache is rebuilt when an entity changes, except with `PAM_ENV=production`.

Migrations are explicit so concurrent production workers never race schema
changes:

```bash
pam composer migrate              # run pending migrations
pam composer migrate generate     # write a migration for changed entities
pam composer migrate rollback 1   # revert the last migration
pam composer migrate status       # list migrations and when they ran
pam composer migrate cache        # rebuild the schema cache during deploy
```

## License

The Nimbus skeleton is open source under the
[Apache License 2.0](LICENSE). Application files copied from this
skeleton and code emitted by PAM generators may be used, modified, sublicensed,
and distributed under terms of your choice, as stated in the Additional Use
Grant.
