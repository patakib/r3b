# r3b - Rust BraveBookmarkBackup
A tiny CLI tool to synchronize your bookmarks between hosts without introducing yet another cloud account into your workflow.

Initially developed for Brave but actually can work for any other browser - if you know the path of the bookmarks used by the browser.

# Configuration
Create a ```config.toml``` file next to the executable with the following fields:

```
```
```
```
```
``````
```
```remote_host = "your_remote_hostname"
remote_user = "your_remote_username"
remote_bookmark_path = "where you store your bookmark file on the remote machine (the backup)"
local_windows_bookmark_path = "your local bookmark file which is used by the browser - on windows"
local_linux_bookmark_path = "your local bookmark file which is used by the browser - on linux""
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
```
