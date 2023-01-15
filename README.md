# Paste

## Simple API
### Retrieve Data
The server decides based on the UserAgent whether to send the data as a raw file or embedded in a website. To differentiated browsers from non browsers it uses the `Mozilla` all modern browsers (even IE) send.

For a browser this will return a HTML with the file embedded and displayed correctly with added controls for e.g. deleting the entry. For all other requests (mainly tools like curl) the file will be returned as raw data.

- get("/:id")      ->  return entry as text
- get("/:id.:ext") ->  return entry as the expect type for the extension
    - for code files this will return highlighted code (as long as it is supported by [syntect](https://github.com/trishume/syntect))
    - for image files it will embed the file in an `<img>` tag to display.

### Delete entry
There is the semantically correct way of deleting data using delete requests, but to make the website work without JS, it also supports using a get endpoint for deletion. The extension is optional and will be ignored.

Deleting a file can either be done by the original uploader, identified via a cookie or a random passphrase imidiatly, as well as anyone viewing it with a 30 minute delay.

- delete("/:id<.:ext>")
- get("delete/:id<.:ext>")

### Add entry
There are multiple ways of to add entries.
- post("/", `body:Form<{file: File, text: String, extension: Option<String>}>`) -> Adds a file or text via the website
- post("/", `body:String`) -> Adds a text entry and returns the URL
- post("/", `body:File`) -> Adds a file entry and returns the URL

The reason `String` and `File` are differentiated is, that there will be features to handle text in better ways and allow some simple serverside transformations.

- remove empty preceding and trailing lines
- remove inner empty lines
- 2D-trim, this will remove the smallest indentation from all lines
- TAB-TO-SPACE, so you get the expected tab width in spaces

There still needs to be found a reasonable way of atteching them to the non form requests. One would be to allow a JSON request with additional fields or add them through headers/cookies.
