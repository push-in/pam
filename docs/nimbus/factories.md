# Factories e seeders

## Factories

Factories montam entidades Arcano com dados do [Faker](https://fakerphp.org):

```bash
pam composer require --dev fakerphp/faker
pam make:factory ProductFactory
```

```php
namespace Database\Factories;

use App\Models\Product;
use App\Models\ProductStatus;
use Nimbus\Database\Factory;

/** @extends Factory<Product> */
final class ProductFactory extends Factory
{
    protected string $model = Product::class;

    public function definition(): array
    {
        return [
            'name' => $this->faker->words(2, true),
            'priceInCents' => $this->faker->numberBetween(100, 100_000),
            'category' => CategoryFactory::new(),   // criada junto e gravada em cascata
        ];
    }

    public function archived(): self
    {
        return $this->state(['status' => ProductStatus::Archived]);
    }
}
```

Atributos com o nome de um parâmetro do construtor vão para o construtor; os demais são atribuídos às propriedades.

```php
ProductFactory::new()->makeOne();                         // não gravada
ProductFactory::new()->count(10)->archived()->create();   // gravadas numa só unidade de trabalho
ProductFactory::new()->sequence(['name' => 'Teclado'], ['name' => 'Mouse'])->count(4)->create();
ProductFactory::new()->state(fn (array $a) => ['name' => strtoupper($a['name'])])->raw();
```

- `count()`, `state()`, `sequence()`, `afterMaking()` e `afterCreating()` devolvem cópias.
- `make()` e `create()` devolvem uma entidade, ou uma lista depois de `count()`. Versões tipadas: `makeOne`, `makeMany`, `createOne`, `createMany`.
- `create()` grava pelo `ArcanoManager` definido em `Factory::useManager()`, que o `TestCase` e o `SeederRunner` configuram.
- `Factory::useFaker()` troca o locale ou a semente do Faker.

## Seeders

Seeders definem `run()`, resolvido pelo container, e chamam outros com `call()`:

```php
namespace Database\Seeders;

use Nimbus\Database\ArcanoManager;
use Nimbus\Database\Seeder;

final class DatabaseSeeder extends Seeder
{
    public function run(ArcanoManager $arcano): void
    {
        $this->call([CategorySeeder::class, ProductSeeder::class]);
    }
}
```

```php
use Nimbus\Database\SeederRunner;

(new SeederRunner($app))->run(DatabaseSeeder::class);
```

No template web, `pam exec bin/migrate seed` roda o `DatabaseSeeder`. Nos testes, use `$this->seed(DatabaseSeeder::class)`.
