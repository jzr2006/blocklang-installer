use std::path::Path;
use std::fs::{self, File};
use std::io::{self, BufReader};
use reqwest;
use zip::ZipArchive;

#[cfg(test)]
use mockito;

#[cfg(not(test))]
const URL: &str = "https://www.blocklang.com";

#[cfg(test)]
const URL: &str = mockito::SERVER_URL;

const ROOT_PATH_SOFTWARE: &str = "softwares";

/// 从软件中心下载软件。
/// 
/// `download` 函数将根据 `software_name` 指定的软件名，
/// `software_version` 指定的软件版本号，从软件发布中心下载软件。
/// 然后将下载的软件存到应用服务器指定的目录中，并将文件名设置为 `software_file_name`。
/// 
/// 如果在指定的文件夹下找到对应的文件，则中断下载，直接使用已存在文件。
/// 
/// 下载完成后，会返回新下载文件的完整路径。
/// 
/// 应用服务器的目录结构为
/// 
/// * softwares
///     * software_name
///         * software_version
///             * software_file_name
/// 
/// # Examples
/// 
/// ```no_run
/// use installer::download;
/// 
/// fn main() -> Result<(), Box<std::error::Error>> {
///     download("app", "0.1.0", "app-0.1.0.zip")?;
///     Ok(())
/// }
/// ```
pub fn download(software_name: &str, 
    software_version: &str, 
    software_file_name: &str) -> Result<String, Box<std::error::Error>> {
    
    let saved_dir_path = &format!("{}/{}/{}", 
        ROOT_PATH_SOFTWARE, 
        software_name, 
        software_version);

    fs::create_dir_all(saved_dir_path)?;

    let saved_file_path = &format!("{}/{}", saved_dir_path, software_file_name);

    let path = Path::new(saved_file_path);
    // 如果文件已存在，则直接返回文件名
    if path.exists() {
        return Ok(saved_file_path.to_string());
    }

    println!("开始下载文件：{}", software_file_name);

    let url = &format!("{}/softwares?name={}&version={}", 
        URL, 
        software_name, 
        software_version);
    let mut response = reqwest::get(url)?;

    if response.status().is_success() {
        println!("返回成功，开始在本地写入文件");
        let mut file = File::create(saved_file_path)?;
        response.copy_to(&mut file)?;
        println!("下载完成。");
    } else {
        println!("出现了其他错误，状态码为：{:?}", response.status());
    }

    Ok(saved_file_path.to_string())
}

/// 将 `source_file_path` 的压缩文件解压到 `target_dir_path` 目录下。
/// 
/// # Examples
/// 
/// ```no_run
/// use installer::unzip_to;
/// 
/// fn main() -> Result<(), Box<std::error::Error>> {
///     unzip_to("test.zip", "another/folder")?;
///     Ok(())
/// }
/// ```
pub fn unzip_to(source_file_path: &str, target_dir_path: &str) -> Result<(), Box<std::error::Error>> {
    let source_path = Path::new(source_file_path);

    let file_name = source_path.file_name().unwrap().to_str().unwrap();
    let target_path = Path::new(target_dir_path).join(file_name);

    let is_in_same_dir = source_path == target_path;

    // 如果源目录跟目标目录相同，则不复制
    if !is_in_same_dir {
        // 将压缩文件复制到指定的目录
        fs::create_dir_all(target_dir_path)?;
        fs::copy(source_path, &target_path)?;
    }

    // 解压文件
    unzip_file(target_path.to_str().unwrap())?;

    // 删除目标文件夹中的压缩文件
    if !is_in_same_dir {
        fs::remove_file(target_path)?;
    }

    Ok(())
}

/// 将压缩文件解压到当前目录，即存放压缩文件的目录中。
/// 
/// 注意：解压完成后，并不会删除之前的压缩文件 `source_file_path`
fn unzip_file(source_file_path: &str) -> Result<(), Box<std::error::Error>> {
    let source_file = File::open(source_file_path)?;
    let source_reader = BufReader::new(source_file);
    let mut archive = ZipArchive::new(source_reader)?;

    // 获取被压缩文件所在的文件夹
    let parent_dir = Path::new(source_file_path).parent().unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let out_path = parent_dir.join(&file.sanitized_name());

        if (&*file.name()).ends_with('/') {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(p) = out_path.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p)?;
                }
            }
            let mut out_file = fs::File::create(&out_path)?;
            io::copy(&mut file, &mut out_file)?;
        }

        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    
    use std::io::Write;
    use std::io::prelude::*;
    use std::fs::{self, File};
    use std::path::Path;
    use tempfile::NamedTempFile;
    use mockito::mock;
    use zip::CompressionMethod::Stored;
    use zip::result::{ZipResult};
    use zip::write::{ZipWriter, FileOptions};
    use super::{download, unzip_to, ROOT_PATH_SOFTWARE};

    const TEMP_FILE_NAME: &str = "hello_world.txt";

    #[test]
    #[should_panic]
    fn download_server_not_work() {
        match download("app", "0.1.0", "app-0.1.0.zip") {
            Err(why) => panic!("{:?}", why),
            _ => (),
        };
    }

    #[test]
    fn download_success() -> Result<(), Box<std::error::Error>> {
        // 创建一个临时文件，当作下载文件
        let mut file = NamedTempFile::new()?;
        writeln!(file, "I am a software!")?;
        let path = file.path();
        let path = path.to_str().unwrap();

        // mock 下载文件的 http 服务
        let mock = mock("GET", "/softwares?name=app&version=0.1.0")
            .with_body_from_file(path)
            .with_status(200)
            .create();
        
        {
            // 执行下载文件方法
            let downloaded_file_path = download("app", "0.1.0", "app-0.1.0.zip")?;

            // 断言文件已下载成功
            assert!(Path::new(&downloaded_file_path).exists());

            // 删除已下载的文件
            fs::remove_dir_all(ROOT_PATH_SOFTWARE)?;
        }

        // 断言已执行过 mock 的 http 服务
        mock.assert();

        Ok(())
    }

    #[test]
    fn unzip_to_success() -> Result<(), Box<std::error::Error>> {
        let zip_file_name = "test.zip";
        // 生成一个 zip 文件
        generate_zip_file(zip_file_name)?;
        // 将文件 test.zip 解压到 test_folder/ 文件夹下
        let target_dir = "test_folder";
        unzip_to(zip_file_name, target_dir)?;

        // 如果不将以下代码放在单独放在一个作用域中，
        // 在执行 `fs::remove_dir_all(target_dir)?;` 时
        // 总是会报“目录不为空”的错误，但实际上已经将目录中的文件删除了
        {
            // 断言文件解压成功
            let unzip_file_path = Path::new(target_dir).join(TEMP_FILE_NAME);
            assert!(unzip_file_path.exists());
            // 读取文件的内容，断言内容为“Hello, World!”
            let mut unzip_file = File::open(&unzip_file_path)?;
            let mut unzip_file_content = String::new();
            unzip_file.read_to_string(&mut unzip_file_content)?;
            assert_eq!(unzip_file_content, "Hello, World!");
        }
        
        // 删除 test.zip 文件
        fs::remove_file(zip_file_name)?;
        // 删除 test_folder 目录
        fs::remove_dir_all(target_dir)?;
        Ok(())
    }

    fn generate_zip_file(zip_file_name: &str) -> ZipResult<()> {
        //  1. 生成一个临时文件
        //  2. 将临时文件压缩成 zip
        let file = File::create(zip_file_name)?;
        let mut zip = ZipWriter::new(file);

        let options = FileOptions::default().compression_method(Stored);
        zip.start_file(TEMP_FILE_NAME, options)?;
        zip.write_all(b"Hello, World!")?;

        zip.finish()?;
        Ok(())
    }
}