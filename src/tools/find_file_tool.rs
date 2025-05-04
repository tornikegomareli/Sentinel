use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use ollama_rs::generation::tools::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

const MAX_OUTPUT_LENGTH: usize = 30000;
const MAX_SEARCH_DEPTH: usize = 10; // Maximum directory depth to search

/// Parameters for the FindAndReadFileTool
#[derive(Deserialize, JsonSchema)]
pub struct FindAndReadFileParams {
    #[schemars(
        description = "The exact name of the file to search for (e.g., 'main.rs', 'README.md')"
    )]
    filename: String,

    #[schemars(
        description = "Optional. The relative path of the directory where the recursive search should begin. Defaults to the current working directory if omitted."
    )]
    search_path: Option<String>,

    #[schemars(
        description = "Optional. Whether to search inside hidden directories (like '.git', '.build'). Defaults to false."
    )]
    include_hidden_dirs: Option<bool>,
}

pub struct FindAndReadFileTool {}

impl Default for FindAndReadFileTool {
    fn default() -> Self {
        Self {}
    }
}

impl FindAndReadFileTool {
    pub fn new() -> Self {
        Self::default()
    }

    fn truncate_output(content: &str) -> String {
        if content.len() <= MAX_OUTPUT_LENGTH {
            return content.to_string();
        }

        let half_length = MAX_OUTPUT_LENGTH / 2;
        let start = &content[..half_length];
        let end = &content[content.len() - half_length..];

        // Count truncated lines
        let middle_content = &content[half_length..content.len() - half_length];
        let truncated_lines_count = middle_content.chars().filter(|&c| c == '\n').count();

        format!(
            "{}\n\n... [{} lines truncated] ...\n\n{}",
            start, truncated_lines_count, end
        )
    }

    // Perform recursive file search
    fn find_file(
        &self,
        filename: &str,
        search_path: &Path,
        include_hidden_dirs: bool,
        depth: usize,
    ) -> Option<PathBuf> {
        // Check maximum search depth to prevent infinite recursion
        if depth > MAX_SEARCH_DEPTH {
            return None;
        }

        // Skip if path doesn't exist or isn't a directory
        if !search_path.exists() || !search_path.is_dir() {
            return None;
        }

        // Try to read directory entries
        let entries = match fs::read_dir(search_path) {
            Ok(entries) => entries,
            Err(e) => {
                println!(
                    "\x1b[1;33m[FIND FILE TOOL] Error reading directory '{}': {}\x1b[0m",
                    search_path.display(),
                    e
                );
                return None;
            }
        };

        // Check each entry
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_name_os = entry.file_name();
            let file_name = match file_name_os.to_str() {
                Some(name) => name,
                None => continue, // Skip entries with invalid Unicode names
            };

            // Skip hidden directories if not included
            if !include_hidden_dirs && file_name.starts_with('.') && path.is_dir() {
                continue;
            }

            // Check if this is the target file
            if file_name == filename && path.is_file() {
                return Some(path);
            }

            // Recursively search subdirectories
            if path.is_dir() {
                if let Some(found_path) =
                    self.find_file(filename, &path, include_hidden_dirs, depth + 1)
                {
                    return Some(found_path);
                }
            }
        }

        None
    }

    async fn find_and_read_file(
        &self,
        params: &FindAndReadFileParams,
    ) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        let filename = &params.filename;
        let include_hidden_dirs = params.include_hidden_dirs.unwrap_or(false);

        // Determine the search root directory
        let search_root = if let Some(search_path) = &params.search_path {
            let path = Path::new(search_path);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                match env::current_dir() {
                    Ok(current_dir) => current_dir.join(path),
                    Err(e) => return Err(format!("Failed to get current directory: {}", e).into()),
                }
            }
        } else {
            match env::current_dir() {
                Ok(current_dir) => current_dir,
                Err(e) => return Err(format!("Failed to get current directory: {}", e).into()),
            }
        };

        // Log search parameters
        println!("\x1b[1;34m[FIND FILE TOOL] Searching for '{}' starting from '{}' (include hidden: {})\x1b[0m",
            filename, search_root.display(), include_hidden_dirs);

        // Perform the recursive search
        if let Some(file_path) = self.find_file(filename, &search_root, include_hidden_dirs, 0) {
            println!(
                "\x1b[1;32m[FIND FILE TOOL] Found '{}' at: {}\x1b[0m",
                filename,
                file_path.display()
            );

            // Read the file content
            match fs::read_to_string(&file_path) {
                Ok(content) => {
                    // Truncate content if necessary
                    let content = Self::truncate_output(&content);
                    Ok(content)
                }
                Err(e) => {
                    Err(format!("Error reading file '{}': {}", file_path.display(), e).into())
                }
            }
        } else {
            Err(format!(
                "File '{}' not found in search path: {}",
                filename,
                search_root.display()
            )
            .into())
        }
    }
}

impl Tool for FindAndReadFileTool {
    type Params = FindAndReadFileParams;

    fn name() -> &'static str {
        "find_file"
    }

    fn description() -> &'static str {
        "Recursively searches for a file by its name within a specified directory (or current directory) and returns the content of the first match found.

WHEN TO USE THIS TOOL:
- When you need to find and read a file by name but don't know its exact location in the project
- When a file might exist in one of several possible directories
- When you want to search the entire project for a specific file

SUPPORTED PARAMETERS:
- 'filename': (REQUIRED) The exact name of the file to search for (e.g., 'main.rs', 'README.md')
- 'search_path': (OPTIONAL) The relative path of the directory where the recursive search should begin. Defaults to the current working directory if omitted.
- 'include_hidden_dirs': (OPTIONAL) Whether to search inside hidden directories (like '.git', '.build'). Defaults to false.

HOW TO USE:
1. Provide the 'filename' parameter with the exact name of the file you're looking for
2. Optionally specify 'search_path' to start the search from a specific directory
3. Optionally set 'include_hidden_dirs' to true if you want to include hidden directories in the search

EXAMPLES:
- To find and read the main.rs file anywhere in the project: { filename: 'main.rs' }
- To search for config.json in the src directory: { filename: 'config.json', search_path: 'src' }
- To find .gitignore including hidden directories: { filename: '.gitignore', include_hidden_dirs: true }

FEATURES:
- Recursive search down to multiple directory levels
- Option to include or exclude hidden directories
- Handles large files by truncating output when necessary
- Provides informative error messages if the file isn't found

LIMITATIONS:
- Search is limited to 10 directory levels deep to prevent excessive recursion
- Output is truncated if it exceeds 30,000 characters
- Searching with 'include_hidden_dirs: true' may be slower
- Matches only by exact filename, not by path patterns or content"
    }

    async fn call(
        &mut self,
        parameters: Self::Params,
    ) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Start timing the execution
        let start_time = Instant::now();

        // Print colorful message indicating tool is being called
        println!(
            "\x1b[1;32m[FIND FILE TOOL] Being called to find file: {}\x1b[0m",
            parameters.filename
        );

        // Execute the find and read operation
        let result = self.find_and_read_file(&parameters).await;

        // Calculate execution time
        let execution_time = start_time.elapsed().as_millis();

        // Return result with execution time
        match result {
            Ok(output) => Ok(format!(
                "{}\n\nOperation completed in {}ms",
                output, execution_time
            )),
            Err(e) => Ok(format!(
                "Error: {}\n\nOperation failed after {}ms",
                e, execution_time
            )),
        }
    }
}

// Interface for our application
pub struct FindFile {
    tool: FindAndReadFileTool,
}

impl FindFile {
    pub fn new() -> Self {
        Self {
            tool: FindAndReadFileTool::new(),
        }
    }

    pub async fn find_and_read(
        &mut self,
        filename: &str,
        search_path: Option<&str>,
        include_hidden_dirs: bool,
    ) -> Result<String> {
        let params = FindAndReadFileParams {
            filename: filename.to_string(),
            search_path: search_path.map(|s| s.to_string()),
            include_hidden_dirs: Some(include_hidden_dirs),
        };

        match self.tool.call(params).await {
            Ok(output) => {
                // Extract the actual file content before the "Operation completed" message
                if let Some(idx) = output.find("\n\nOperation completed") {
                    Ok(output[..idx].to_string())
                } else {
                    Ok(output)
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to find and read file: {}", e)),
        }
    }
}
