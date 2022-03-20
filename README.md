# Paste

## Simple API
### Retrieve Data
The server will based on the user agent decide whether to send the data as a raw file. 

As for some reason all browsers send `Mozilla` this is going to be used for now.

For a browser this will return a HTML with the file embeded with added controlls for e.g. delete the entry. For all other requests (mainly things like curl) the file will be returned as raw data.

- get("/:id")       ->  return entry as text
- get("/:id.bin")   ->  return entry as binary
- get("/:id.:ext")  ->  return entry as the expect type for the extension

### Delete entry
There is the semantically correct way of deleting data using delete requests, but to make the website work without JS because there are people like that, it also supports using ?delete with a get request.

- delete("/:id")
- get("/:id?delete")

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
