use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(command: &str, root: &Path, arguments: Vec<OsString>) -> Result<u8, String> {
    let name = parse_name(arguments)?;
    let (directory, file_name, source) = match command {
        "make:model" => ("app/Models", format!("{name}.php"), model(&name)),
        "make:controller" => ("app/Controllers", format!("{name}.php"), controller(&name)),
        "make:request" => ("app/Requests", format!("{name}.php"), request(&name)),
        "make:resource" => ("app/Resources", format!("{name}.php"), resource(&name)),
        "make:service" => ("app/Services", format!("{name}.php"), service(&name)),
        "make:repository" => ("app/Repositories", format!("{name}.php"), repository(&name)),
        "make:migration" => {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "system clock is before the Unix epoch".to_owned())?
                .as_secs();
            (
                "database/migrations",
                format!(
                    "{}_0_0_default_{}.php",
                    migration_date(timestamp),
                    snake_case(&name)
                ),
                migration(&name, timestamp),
            )
        }
        _ => return Err(format!("unsupported PAM API generator {command}")),
    };
    let path = root.join(directory).join(file_name);
    write_new(&path, source.as_bytes())?;
    println!("Created {}", path.display());
    Ok(0)
}

fn parse_name(arguments: Vec<OsString>) -> Result<String, String> {
    let mut arguments = arguments.into_iter();
    let name = arguments
        .next()
        .ok_or_else(|| "generator commands require a PascalCase name".to_owned())?
        .into_string()
        .map_err(|_| "generator names must be valid UTF-8".to_owned())?;
    if arguments.next().is_some() {
        return Err("PAM API generators accept exactly one name".to_owned());
    }
    if name.len() > 80
        || !name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(
            "generator names must start uppercase and contain only ASCII letters or digits"
                .to_owned(),
        );
    }
    Ok(name)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "generated file has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "cannot create {} without overwriting: {error}",
                path.display()
            )
        })?;
    file.write_all(bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn prelude(namespace: &str) -> String {
    format!("<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\{namespace};\n\n")
}

fn model(name: &str) -> String {
    let table = format!("{}s", snake_case(name));
    format!(
        "{}use Arcano\\Annotated\\Annotation\\Column;\nuse Arcano\\Annotated\\Annotation\\Entity;\n\n#[Entity(role: '{role}', table: '{table}')]\nclass {name}\n{{\n    #[Column(type: 'primary')]\n    public ?int \u{24}id = null;\n}}\n",
        prelude("Models"),
        role = snake_case(name),
    )
}

fn controller(name: &str) -> String {
    format!(
        "{}use Pam\\Http\\Request;\nuse Pam\\Http\\Response;\n\nfinal readonly class {name}\n{{\n    public function index(Request \u{24}request, Response \u{24}response): Response\n    {{\n        return \u{24}response->json(['data' => []]);\n    }}\n}}\n",
        prelude("Controllers")
    )
}

fn request(name: &str) -> String {
    format!(
        "{}use Nimbus\\Validation\\FormRequest;\n\nfinal class {name} extends FormRequest\n{{\n    public function rules(): array\n    {{\n        return [];\n    }}\n}}\n",
        prelude("Requests")
    )
}

fn resource(name: &str) -> String {
    format!(
        "{}use Nimbus\\Http\\JsonResource;\nuse Pam\\Http\\Request;\n\nfinal readonly class {name} extends JsonResource\n{{\n    public function toArray(Request \u{24}request): array\n    {{\n        return [];\n    }}\n}}\n",
        prelude("Resources")
    )
}

fn service(name: &str) -> String {
    format!(
        "{}final readonly class {name}\n{{\n}}\n",
        prelude("Services")
    )
}

fn repository(name: &str) -> String {
    format!(
        "{}final readonly class {name}\n{{\n}}\n",
        prelude("Repositories")
    )
}

fn migration(name: &str, timestamp: u64) -> String {
    let table = snake_case(name.trim_start_matches("Create").trim_end_matches("Table"));
    format!(
        "<?php\n\ndeclare(strict_types=1);\n\nnamespace Migration;\n\nuse Arcano\\Migrations\\Migration;\n\nclass {name}{timestamp} extends Migration\n{{\n    protected const DATABASE = 'default';\n\n    public function up(): void\n    {{\n        \u{24}this->table('{table}')\n            ->addColumn('id', 'primary')\n            ->addColumn('created_at', 'datetime')\n            ->addColumn('updated_at', 'datetime')\n            ->setPrimaryKeys(['id'])\n            ->create();\n    }}\n\n    public function down(): void\n    {{\n        \u{24}this->table('{table}')->drop();\n    }}\n}}\n"
    )
}

/// `YYYYMMDD.HHMMSS` in UTC, the prefix Arcano migrations are ordered by.
fn migration_date(timestamp: u64) -> String {
    let days = (timestamp / 86_400) as i64;
    let seconds = timestamp % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}.{:02}{:02}{:02}",
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60
    )
}

fn snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 8);
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_without_overwriting() {
        let root = std::env::temp_dir().join(format!(
            "pam-api-generator-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        run(
            "make:controller",
            &root,
            vec![OsString::from("LoginController")],
        )
        .unwrap();
        let path = root.join("app/Controllers/LoginController.php");
        let source = fs::read_to_string(&path).unwrap();
        assert!(source.contains("final readonly class LoginController"));
        assert!(
            run(
                "make:controller",
                &root,
                vec![OsString::from("LoginController")]
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generates_arcano_models_and_migrations() {
        let model = model("OrderItem");
        assert!(model.contains("#[Entity(role: 'order_item', table: 'order_items')]"));
        assert!(model.contains("use Arcano\\Annotated\\Annotation\\Column;"));
        let migration = migration("CreateOrderItemsTable", 1_763_510_400);
        assert!(migration.contains("extends Migration"));
        assert!(migration.contains("$this->table('order_items')"));
        assert_eq!(migration_date(1_763_510_400), "20251119.000000");
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(parse_name(vec![OsString::from("../Secret")]).is_err());
    }
}
