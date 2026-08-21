# Pam application

> This branch targets PAM API 2.0. Use the published `1.x` skeleton until the
> PAM API 2.0 prerelease is available on Packagist.

```bash
pam composer install
mkdir -p storage && touch storage/database.sqlite
pam composer migrate
pam composer dev
```

The API starts at `http://127.0.0.1:3000`.

The starter intentionally demonstrates the structured PAM API path: a named
controller method orchestrates a service and returns a JSON Resource. Closures
remain available for small endpoints, but application rules should live in
services and persistence should live in repositories.

It also ships an executable Eloquent vertical slice:

```bash
curl -X POST http://127.0.0.1:3000/api/products \
  -H 'content-type: application/json' \
  -d '{"name":"Mechanical keyboard","priceInCents":34990}'
curl http://127.0.0.1:3000/api/products
```

`ProductController` stays thin, `StoreProductRequest` validates and hydrates a
typed DTO, `ProductService` owns the use case, `ProductRepository` isolates
Eloquent persistence, and `ProductResource` owns the response contract. Product
statuses are sequential integer-backed enum values (`1` active, `2` archived).
Creation returns `201 Created`; resources may select any valid HTTP status while
retaining PAM API's consistent `data` envelope.

Run the in-memory application test inside Pam's Embed SAPI:

```bash
pam composer test
```

The generated application and this published skeleton share the same source
files:

```text
index.php
src/
├── Domain/Products/
├── Http/{Controllers,Requests,Resources}/
├── Models/Product.php
├── Providers/AppServiceProvider.php
├── Repositories/
└── Services/
database/migrations/
tests/{ApplicationTest.php,bootstrap.php}
```

`index.php` validates typed configuration before listening, installs secure
response headers, and boots Eloquent from environment configuration. Migrations
are explicit so concurrent production workers never race schema changes.

## License

The PAM skeleton is open source under the
[Apache License 2.0](LICENSE). Application files copied from this
skeleton and code emitted by PAM generators may be used, modified, sublicensed,
and distributed under terms of your choice, as stated in the Additional Use
Grant.
