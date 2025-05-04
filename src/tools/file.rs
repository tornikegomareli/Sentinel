use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::env;

use anyhow::Result;
use ollama_rs::generation::tools::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;

const MAX_OUTPUT_LENGTH: usize = 30000;

// Removed the individual parameter structs as they are now merged into FileParams

#[derive(Deserialize, JsonSchema)]
pub struct FileParams {
    #[schemars(description = "The operation to perform: 'read', 'write', 'exists', 'delete', 'move', or 'copy'")]
    operation: Option<String>,
    
    #[schemars(description = "The path to the file to read, write, check, or delete")]
    path: Option<String>,
    
    #[schemars(description = "The content to write to the file (for write operation)")]
    content: Option<String>,
    
    #[schemars(description = "Whether to append to the file instead of overwriting it (for write operation)")]
    append: Option<bool>,
    
    #[schemars(description = "The source path for move or copy operations")]
    source: Option<String>,
    
    #[schemars(description = "The destination path for move or copy operations")]
    destination: Option<String>,
}

pub struct FileTool {
}

impl Default for FileTool {
    fn default() -> Self {
        Self {}
    }
}

impl FileTool {
    pub fn new() -> Self {
        Self::default()
    }
    
    // Helper function to ensure paths are absolute
    fn resolve_path(&self, path_str: &str) -> Result<PathBuf, Box<dyn std::error::Error + Sync + Send>> {
        let path = Path::new(path_str);
        
        // If already absolute, return it
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        
        // Otherwise, make it absolute by prepending the current working directory
        match env::current_dir() {
            Ok(current_dir) => {
                let absolute_path = current_dir.join(path);
                println!("\x1b[1;33m[FILE TOOL] Converting relative path '{}' to absolute path '{}'\x1b[0m", 
                    path_str, absolute_path.display());
                Ok(absolute_path)
            },
            Err(e) => Err(format!("Failed to get current directory: {}", e).into()),
        }
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

    async fn read_file(&self, path_str: &str) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute path
        let path = self.resolve_path(path_str)?;
        
        if !path.exists() {
            return Err(format!("Error: File '{}' does not exist", path.display()).into());
        }
        
        if !path.is_file() {
            return Err(format!("Error: Path '{}' is not a file", path.display()).into());
        }
        
        match fs::read_to_string(&path) {
            Ok(content) => Ok(Self::truncate_output(&content)),
            Err(e) => Err(format!("Error reading file: {}", e).into()),
        }
    }
    
    async fn write_file(&self, path_str: &str, content: &str, append: bool) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute path
        let path = self.resolve_path(path_str)?;
        
        // Make sure the parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        
        let mut file = if append {
            TokioFile::options().append(true).create(true).open(&path).await?
        } else {
            TokioFile::create(&path).await?
        };
        
        file.write_all(content.as_bytes()).await?;
        file.flush().await?; // Ensure content is written to disk
        
        Ok(format!("Successfully {} file: {}", 
            if append { "appended to" } else { "wrote" }, 
            path.display()
        ))
    }
    
    async fn file_exists(&self, path_str: &str) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute path
        let path = self.resolve_path(path_str)?;
        let exists = path.exists();
        
        Ok(format!("Path '{}' {} exist", 
            path.display(),
            if exists { "does" } else { "does not" }
        ))
    }
    
    async fn delete_file(&self, path_str: &str) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute path
        let path = self.resolve_path(path_str)?;
        
        if !path.exists() {
            return Err(format!("Error: Path '{}' does not exist", path.display()).into());
        }
        
        if path.is_file() {
            fs::remove_file(&path)?;
            Ok(format!("Successfully deleted file: {}", path.display()))
        } else if path.is_dir() {
            fs::remove_dir_all(&path)?;
            Ok(format!("Successfully deleted directory: {}", path.display()))
        } else {
            Err(format!("Error: Path '{}' is neither a file nor a directory", path.display()).into())
        }
    }
    
    async fn move_file(&self, source_str: &str, destination_str: &str) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute paths
        let source_path = self.resolve_path(source_str)?;
        let dest_path = self.resolve_path(destination_str)?;
        
        if !source_path.exists() {
            return Err(format!("Error: Source path '{}' does not exist", source_path.display()).into());
        }
        
        // Make sure the parent directory of the destination exists
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        
        fs::rename(&source_path, &dest_path)?;
        
        Ok(format!("Successfully moved from '{}' to '{}'", 
            source_path.display(), 
            dest_path.display()
        ))
    }
    
    async fn copy_file(&self, source_str: &str, destination_str: &str) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Resolve to absolute paths
        let source_path = self.resolve_path(source_str)?;
        let dest_path = self.resolve_path(destination_str)?;
        
        if !source_path.exists() {
            return Err(format!("Error: Source path '{}' does not exist", source_path.display()).into());
        }
        
        // Make sure the parent directory of the destination exists
        if let Some(parent) = dest_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        
        if source_path.is_file() {
            fs::copy(&source_path, &dest_path)?;
            Ok(format!("Successfully copied file from '{}' to '{}'", 
                source_path.display(), 
                dest_path.display()
            ))
        } else if source_path.is_dir() {
            copy_dir_all(&source_path, &dest_path)?;
            Ok(format!("Successfully copied directory from '{}' to '{}'", 
                source_path.display(), 
                dest_path.display()
            ))
        } else {
            Err(format!("Error: Source path '{}' is neither a file nor a directory", source_path.display()).into())
        }
    }
}

// Helper function to recursively copy directories
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        
        let new_dst = dst.join(entry.file_name());
        
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &new_dst)?;
        } else {
            fs::copy(entry.path(), new_dst)?;
        }
    }
    
    Ok(())
}

impl Tool for FileTool {
    type Params = FileParams;

    fn name() -> &'static str {
        "file"
    }

    fn description() -> &'static str {
        "File operations tool to read, write, check existence, delete, move, and copy files.

WHEN TO USE THIS TOOL:
- When you need to perform file operations such as reading, writing, checking if a file exists,
  deleting, moving, or copying files
- Useful for managing files within the filesystem

SUPPORTED OPERATIONS (must use exactly these keywords):
- 'read' - Read content from a file
- 'write' - Write content to a file (creates a new file or overwrites existing one)
- 'exists' - Check if a file or directory exists
- 'delete' - Delete a file or directory
- 'move' - Move/rename a file or directory
- 'copy' - Copy a file or directory

HOW TO USE:
1. Set the 'operation' parameter to one of the values above (e.g., 'write' not 'create')
2. Provide the required parameters for the chosen operation:
   - For read: 'path' to the file
   - For write: 'path' to the file and 'content' to write (with optional 'append' flag set to true/false)
   - For exists: 'path' to check
   - For delete: 'path' to the file to delete
   - For move: 'source' and 'destination' paths
   - For copy: 'source' and 'destination' paths

EXAMPLES:
- To create a new file: use operation='write' with path and content parameters
- To check if a file exists: use operation='exists' with path parameter
- To rename a file: use operation='move' with source and destination parameters

FEATURES:
- Supports multiple file operations
- Can handle both files and directories
- Creates parent directories if they don't exist when writing or copying files
- Handles large files by truncating output when necessary

LIMITATIONS:
- Output is truncated if it exceeds 30,000 characters
- For security reasons, restricted to standard file operations
- Cannot access system-protected files or directories

TIPS:
- Use the 'exists' operation to check if a file exists before attempting to read or modify it
- Use the 'append' option with the 'write' operation to add content to existing files
- The 'move' operation can also be used to rename files"
    }

    async fn call(
        &mut self,
        parameters: Self::Params,
    ) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        // Start timing the execution
        let start_time = Instant::now();
        
        // Get operation type
        let operation = parameters.operation.as_deref().unwrap_or("").to_lowercase();
        
        // Print colorful message indicating tool is being called
        println!("\x1b[1;32m[FILE TOOL] Being called with operation: {}\x1b[0m", operation);
        
        // Log all parameters for debugging
        let content_str = if let Some(content) = &parameters.content {
            if content.len() > 30 {
                format!("[{} chars]", content.len())
            } else {
                format!("{:?}", content)
            }
        } else {
            "None".to_string()
        };
        
        println!("\x1b[1;34m[FILE TOOL DEBUG] Parameters received: operation={:?}, path={:?}, content={}, append={:?}, source={:?}, destination={:?}\x1b[0m", 
            parameters.operation, 
            parameters.path,
            content_str,
            parameters.append,
            parameters.source,
            parameters.destination
        );
            
        // Process the request based on the operation
        let result = match operation.as_str() {
            "read" => {
                if let Some(path) = parameters.path.as_ref() {
                    self.read_file(path).await
                } else {
                    Err(format!("ERROR: Path is required for 'read' operation. Example: {{ operation: 'read', path: '/full/path/to/file.txt' }}").into())
                }
            },
            "write" => {
                match (parameters.path.as_ref(), parameters.content.as_ref()) {
                    (Some(path), Some(content)) => {
                        self.write_file(path, content, parameters.append.unwrap_or(false)).await
                    },
                    (None, Some(_)) => Err(format!("ERROR: Missing 'path' parameter. Example: {{ operation: 'write', path: '/full/path/to/file.txt', content: 'file content' }}").into()),
                    (Some(_), None) => Err(format!("ERROR: Missing 'content' parameter. Example: {{ operation: 'write', path: '/full/path/to/file.txt', content: 'file content' }}").into()),
                    _ => Err(format!("ERROR: Both 'path' and 'content' are required for 'write' operation. Example: {{ operation: 'write', path: '/full/path/to/file.txt', content: 'file content' }}").into())
                }
            },
            "exists" => {
                if let Some(path) = parameters.path.as_ref() {
                    self.file_exists(path).await
                } else {
                    Err(format!("ERROR: Path is required for 'exists' operation. Example: {{ operation: 'exists', path: '/full/path/to/file.txt' }}").into())
                }
            },
            "delete" => {
                if let Some(path) = parameters.path.as_ref() {
                    self.delete_file(path).await
                } else {
                    Err(format!("ERROR: Path is required for 'delete' operation. Example: {{ operation: 'delete', path: '/full/path/to/file.txt' }}").into())
                }
            },
            "move" => {
                match (parameters.source.as_ref(), parameters.destination.as_ref()) {
                    (Some(source), Some(destination)) => {
                        self.move_file(source, destination).await
                    },
                    (None, Some(_)) => Err(format!("ERROR: Missing 'source' parameter. Example: {{ operation: 'move', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into()),
                    (Some(_), None) => Err(format!("ERROR: Missing 'destination' parameter. Example: {{ operation: 'move', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into()),
                    _ => Err(format!("ERROR: Both 'source' and 'destination' are required for 'move' operation. Example: {{ operation: 'move', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into())
                }
            },
            "copy" => {
                match (parameters.source.as_ref(), parameters.destination.as_ref()) {
                    (Some(source), Some(destination)) => {
                        self.copy_file(source, destination).await
                    },
                    (None, Some(_)) => Err(format!("ERROR: Missing 'source' parameter. Example: {{ operation: 'copy', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into()),
                    (Some(_), None) => Err(format!("ERROR: Missing 'destination' parameter. Example: {{ operation: 'copy', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into()),
                    _ => Err(format!("ERROR: Both 'source' and 'destination' are required for 'copy' operation. Example: {{ operation: 'copy', source: '/path/to/source.txt', destination: '/path/to/dest.txt' }}").into())
                }
            },
            "" => Err("ERROR: 'operation' parameter is required. Valid operations are: 'read', 'write', 'exists', 'delete', 'move', 'copy'".into()),
            _ => Err(format!("ERROR: Unknown operation: '{}'. Valid operations are: 'read', 'write', 'exists', 'delete', 'move', 'copy'", operation).into())
        };
        
        // Calculate execution time
        let execution_time = start_time.elapsed().as_millis();
        
        // Return result with execution time
        match result {
            Ok(output) => {
                if output.is_empty() {
                    Ok(format!("File operation completed in {}ms (no output)", execution_time))
                } else {
                    Ok(format!("{}\n\nOperation completed in {}ms", output, execution_time))
                }
            },
            Err(e) => Ok(format!("Error: {}\n\nOperation failed after {}ms", e, execution_time)),
        }
    }
}

// Interface for our application
pub struct File {
    file_tool: FileTool,
}

impl File {
    pub fn new() -> Self {
        Self { file_tool: FileTool::new() }
    }
    
    pub async fn read(&mut self, path: &str) -> Result<String> {
        let params = FileParams {
            operation: Some("read".to_string()),
            path: Some(path.to_string()),
            content: None,
            append: None,
            source: None,
            destination: None,
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => {
                // Extract the actual file content before the "Operation completed" message
                if let Some(idx) = output.find("\n\nOperation completed") {
                    Ok(output[..idx].to_string())
                } else {
                    Ok(output)
                }
            },
            Err(e) => Err(anyhow::anyhow!("Failed to read file: {}", e)),
        }
    }
    
    pub async fn write(&mut self, path: &str, content: &str, append: bool) -> Result<String> {
        let params = FileParams {
            operation: Some("write".to_string()),
            path: Some(path.to_string()),
            content: Some(content.to_string()),
            append: Some(append),
            source: None,
            destination: None,
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow::anyhow!("Failed to write file: {}", e)),
        }
    }
    
    pub async fn exists(&mut self, path: &str) -> Result<bool> {
        let params = FileParams {
            operation: Some("exists".to_string()),
            path: Some(path.to_string()),
            content: None,
            append: None,
            source: None,
            destination: None,
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => Ok(output.contains("does exist")),
            Err(e) => Err(anyhow::anyhow!("Failed to check file existence: {}", e)),
        }
    }
    
    pub async fn delete(&mut self, path: &str) -> Result<String> {
        let params = FileParams {
            operation: Some("delete".to_string()),
            path: Some(path.to_string()),
            content: None,
            append: None,
            source: None,
            destination: None,
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow::anyhow!("Failed to delete file: {}", e)),
        }
    }
    
    pub async fn r#move(&mut self, source: &str, destination: &str) -> Result<String> {
        let params = FileParams {
            operation: Some("move".to_string()),
            path: None,
            content: None,
            append: None,
            source: Some(source.to_string()),
            destination: Some(destination.to_string()),
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow::anyhow!("Failed to move file: {}", e)),
        }
    }
    
    pub async fn copy(&mut self, source: &str, destination: &str) -> Result<String> {
        let params = FileParams {
            operation: Some("copy".to_string()),
            path: None,
            content: None,
            append: None,
            source: Some(source.to_string()),
            destination: Some(destination.to_string()),
        };
        
        match self.file_tool.call(params).await {
            Ok(output) => Ok(output),
            Err(e) => Err(anyhow::anyhow!("Failed to copy file: {}", e)),
        }
    }
}

// Include tests module
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_file_read_write() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        let file_path = dir.path().join("test.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        
        // Test writing to a file
        let content = "Hello, world!";
        let write_result = file_tool.write(&file_path, content, false).await?;
        assert!(write_result.contains("Successfully wrote file"));
        
        // Test reading from the file
        let read_result = file_tool.read(&file_path).await?;
        assert!(read_result.contains("Hello, world!"));
        
        // Test appending to the file
        let append_result = file_tool.write(&file_path, "\nMore content", true).await?;
        assert!(append_result.contains("Successfully appended to file"));
        
        // Read the file again to confirm appending worked
        let read_result = file_tool.read(&file_path).await?;
        assert!(read_result.contains("Hello, world!"));
        assert!(read_result.contains("More content"));
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_file_exists() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        
        let file_path = dir.path().join("test.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let nonexistent_path = dir.path().join("nonexistent.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        
        // Create a test file
        let _ = file_tool.write(&file_path, "Test content", false).await?;
        
        // Test file exists
        let exists = file_tool.exists(&file_path).await?;
        assert!(exists);
        
        // Test file doesn't exist
        let exists = file_tool.exists(&nonexistent_path).await?;
        assert!(!exists);
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_file_delete() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        
        let file_path = dir.path().join("test.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        
        // Create a test file
        let _ = file_tool.write(&file_path, "Test content", false).await?;
        
        // Confirm file exists
        let exists = file_tool.exists(&file_path).await?;
        assert!(exists);
        
        // Delete the file
        let delete_result = file_tool.delete(&file_path).await?;
        assert!(delete_result.contains("Successfully deleted file"));
        
        // Confirm file no longer exists
        let exists = file_tool.exists(&file_path).await?;
        assert!(!exists);
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_file_move() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        
        let source_path = dir.path().join("source.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let dest_path = dir.path().join("dest.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        
        // Create a test file
        let _ = file_tool.write(&source_path, "Test content", false).await?;
        
        // Move the file
        let move_result = file_tool.r#move(&source_path, &dest_path).await?;
        assert!(move_result.contains("Successfully moved"));
        
        // Confirm source no longer exists
        let source_exists = file_tool.exists(&source_path).await?;
        assert!(!source_exists);
        
        // Confirm destination exists
        let dest_exists = file_tool.exists(&dest_path).await?;
        assert!(dest_exists);
        
        // Confirm content was preserved
        let read_result = file_tool.read(&dest_path).await?;
        assert!(read_result.contains("Test content"));
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_file_copy() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        
        let source_path = dir.path().join("source.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let dest_path = dir.path().join("dest.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        
        // Create a test file
        let _ = file_tool.write(&source_path, "Test content", false).await?;
        
        // Copy the file
        let copy_result = file_tool.copy(&source_path, &dest_path).await?;
        assert!(copy_result.contains("Successfully copied file"));
        
        // Confirm source still exists
        let source_exists = file_tool.exists(&source_path).await?;
        assert!(source_exists);
        
        // Confirm destination exists
        let dest_exists = file_tool.exists(&dest_path).await?;
        assert!(dest_exists);
        
        // Confirm content was copied
        let read_result = file_tool.read(&dest_path).await?;
        assert!(read_result.contains("Test content"));
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_directory_copy() -> anyhow::Result<()> {
        let mut file_tool = File::new();
        // Store tempdir in a variable that lives for the entire test
        let dir = tempdir()?;
        
        // Create a test directory structure
        let source_dir = dir.path().join("source_dir");
        let dest_dir = dir.path().join("dest_dir");
        
        fs::create_dir(&source_dir)?;
        
        let source_file = source_dir.join("test.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let _ = file_tool.write(&source_file, "Test content", false).await?;
        
        // Convert paths to strings safely
        let source_dir_str = source_dir.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let dest_dir_str = dest_dir.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
            
        // Copy the directory
        let copy_result = file_tool.copy(&source_dir_str, &dest_dir_str).await?;
        
        assert!(copy_result.contains("Successfully copied directory"));
        
        // Confirm the file was copied in the destination directory
        let dest_file = dest_dir.join("test.txt").to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in path"))?
            .to_string();
        let dest_exists = file_tool.exists(&dest_file).await?;
        assert!(dest_exists);
        
        // Confirm content was copied
        let read_result = file_tool.read(&dest_file).await?;
        assert!(read_result.contains("Test content"));
        
        // Keep dir alive until end of test
        drop(dir);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_truncate_output() {
        // Generate a string longer than MAX_OUTPUT_LENGTH
        let long_string = "A".repeat(MAX_OUTPUT_LENGTH + 10000);
        
        let truncated = FileTool::truncate_output(&long_string);
        
        // The truncated string should be shorter than the original
        assert!(truncated.len() < long_string.len());
        
        // The truncated string should contain the truncation notice
        assert!(truncated.contains("lines truncated"));
    }
}