use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(command: &str, root: &Path, arguments: Vec<OsString>) -> Result<u8, String> {
    let name = parse_name(arguments)?;
    let (directory, file_name, source) = match command {
        "make:model" => ("src/Models", format!("{name}.php"), model(&name)),
        "make:controller" => (
            "src/Http/Controllers",
            format!("{name}.php"),
            controller(&name),
        ),
        "make:request" => ("src/Http/Requests", format!("{name}.php"), request(&name)),
        "make:resource" => ("src/Http/Resources", format!("{name}.php"), resource(&name)),
        "make:service" => ("src/Services", format!("{name}.php"), service(&name)),
        "make:repository" => ("src/Repositories", format!("{name}.php"), repository(&name)),
        "make:migration" => {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "system clock is before the Unix epoch".to_owned())?
                .as_secs();
            (
                "database/migrations",
                format!("{timestamp}_{}.php", snake_case(&name)),
                migration(&name),
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
    format!(
        "{}use Illuminate\\Database\\Eloquent\\Model;\n\nfinal class {name} extends Model\n{{\n    /** @var list<string> */\n    protected \u{24}fillable = [];\n}}\n",
        prelude("Models")
    )
}

fn controller(name: &str) -> String {
    format!(
        "{}use Pam\\Http\\Request;\nuse Pam\\Http\\Response;\n\nfinal readonly class {name}\n{{\n    public function index(Request \u{24}request, Response \u{24}response): Response\n    {{\n        return \u{24}response->json(['data' => []]);\n    }}\n}}\n",
        prelude("Http\\Controllers")
    )
}

fn request(name: &str) -> String {
    format!(
        "{}use Pam\\Api\\Validation\\FormRequest;\n\nfinal class {name} extends FormRequest\n{{\n    public function rules(): array\n    {{\n        return [];\n    }}\n}}\n",
        prelude("Http\\Requests")
    )
}

fn resource(name: &str) -> String {
    format!(
        "{}use Pam\\Api\\Http\\JsonResource;\nuse Pam\\Http\\Request;\n\nfinal readonly class {name} extends JsonResource\n{{\n    public function toArray(Request \u{24}request): array\n    {{\n        return [];\n    }}\n}}\n",
        prelude("Http\\Resources")
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

fn migration(name: &str) -> String {
    let table = snake_case(name.trim_start_matches("Create").trim_end_matches("Table"));
    format!(
        "<?php\n\ndeclare(strict_types=1);\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{{\n    public function up(): void\n    {{\n        Schema::create('{table}', static function (Blueprint \u{24}table): void {{\n            \u{24}table->id();\n            \u{24}table->timestamps();\n        }});\n    }}\n\n    public function down(): void\n    {{\n        Schema::dropIfExists('{table}');\n    }}\n}};\n"
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
        let path = root.join("src/Http/Controllers/LoginController.php");
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
    fn rejects_path_traversal() {
        assert!(parse_name(vec![OsString::from("../Secret")]).is_err());
    }
}
