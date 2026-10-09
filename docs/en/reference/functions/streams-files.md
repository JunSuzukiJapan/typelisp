<!-- translated-from: docs/ja/reference/functions/streams-files.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# Streams and Files

Stream traits and methods, concrete stream types, file operations and pathnames. Network sockets are
streams too, and are covered in [Networking](network.md).

## 1. The trait hierarchy

What CL expresses with a class hierarchy is expressed here with a **trait hierarchy**. Both the
direction (input / output) and the element type are decided **statically**, so there is no need to ask
at run time "can this stream be read?".

```lisp
(deftrait Stream ()             (open-stream-p ...) (close ...))
(deftrait InputStream  (Stream) (type Item) (read-item ...))
(deftrait OutputStream (Stream) (type Item) (write-item ...))
(deftrait CharInput  ((InputStream  (Item char))) ...)   ; character input
(deftrait CharOutput ((OutputStream (Item char))) ...)   ; character output
(deftrait PeekInput  (CharInput) (unread-char ...) (peek-char ...))  ; input that can push back one character
(deftrait ByteInput  ((InputStream  (Item int))) ...)    ; byte input
(deftrait ByteOutput ((OutputStream (Item int))) ...)    ; byte output
```

A function that reads characters accepts any stream type, built-in or user-defined, if it takes
`(where (CharInput S))` or `:dyn CharInput`.

## 2. Methods

Every method of `CharInput` has a default implementation. An implementation only writes `read-item`.

| Name | Form | Type | Description |
|---|---|---|---|
| `read-item` | `(read-item s)` | `(S)→Option<Item>` | The next element. `none` at the end. **The only method that must be implemented** |
| `read-char` | `(read-char s)` | `(S)→Option<char>` | The next character |
| `read-line` | `(read-line s)` | `(S)→Option<string>` | Up to the next newline (the newline is consumed and removed). A last line that does not end in a newline is returned too |
| `read-all` | `(read-all s)` | `(S)→string` | Everything that is left |
| `read-char-no-hang` | `(read-char-no-hang s)` | `(S)→Option<char>` | Only a character that is already at hand. `none` rather than waiting |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<char>,int)→int` | Pushes up to `n` characters onto `v` and returns how many were actually read. Fewer than `n` only at the end |

`listen` is in `InputStream` (the parent of `CharInput`):

| Name | Form | Type | Description |
|---|---|---|---|
| `listen` | `(listen s)` | `(S)→bool` | Whether the next read can be answered without waiting. The default is `false`, **the side that is never a lie**: `true` would be a guess, and a wrong guess would make `read-char-no-hang` block. All built-in streams override it. **For user-defined streams that do not override it, `read-char-no-hang` always returns `none`** |

`PeekInput` (which inherits from `CharInput`) adds **pushing back one character**. Only the stream
itself has a place to keep the pushed-back character, so this cannot have a default implementation and
is a separate trait. `file-stream`/`string-input-stream`/`standard-stream` implement it, and any other
stream gets it when wrapped with `make-peek-stream` (chapter 4).

| Name | Form | Type | Description |
|---|---|---|---|
| `unread-char` | `(unread-char s c)` | `(S,char)→()` | Makes the next read return `c`. **The only method that must be implemented**. As in CL, only one character is guaranteed |
| `peek-char` | `(peek-char s)` | `(S)→Option<char>` | Looks at the next character without consuming it |

Likewise, for `CharOutput` an implementation only writes `write-item`.

| Name | Form | Type | Description |
|---|---|---|---|
| `write-item` | `(write-item s x)` | `(S,Item)→()` | Writes one element. **The only method that must be implemented** |
| `write-char` | `(write-char s c)` | `(S,char)→()` | Writes one character |
| `write-string` | `(write-string s str)` | `(S,string)→()` | Writes a string |
| `write-line` | `(write-line s str)` | `(S,string)→()` | A string and a newline |
| `terpri` | `(terpri s)` | `(S)→()` | One newline (CL's name) |
| `fresh-line` | `(fresh-line s)` | `(S)→()` | One newline unless at the start of a line |
| `at-line-start` | `(at-line-start s)` | `(S)→bool` | Whether the next character written will start a line. The default is `false` (so `fresh-line` writes the newline: when in doubt, writing is the safe side). All built-in streams override it |
| `finish-output` | `(finish-output s)` | `(S)→()` | Flushes the buffer |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<char>)→()` | Writes all the characters of `v` in order |

`at-line-start` remembers **only what was written through that stream**. `print`/`println`/
`(format true ...)` write to standard output without going through `*standard-output*`, so if you mix
the two, `(fresh-line *standard-output*)` does not know about the newlines `println` wrote. Stick to
one of them.

`Stream` is common to all streams:

| Name | Form | Type | Description |
|---|---|---|---|
| `open-stream-p` | `(open-stream-p s)` | `(S)→bool` | Whether it is still open |
| `close` | `(close s)` | `(S)→()` | Closes it. **The GC does not close streams**, so do it explicitly (or with `with-open-file`) |

## 3. Concrete stream types

| Type | How to make one | Implemented traits |
|---|---|---|
| `file-stream` | `(open-file name direction)` / `open-input` / `open-output` | `CharInput` `PeekInput` `CharOutput` |
| `string-input-stream` | `(make-string-input-stream s)` | `CharInput` `PeekInput` |
| `string-output-stream` | `(make-string-output-stream)` | `CharOutput` |
| `standard-stream` | `*standard-input*` `*standard-output*` `*error-output*` | `CharInput` `PeekInput` `CharOutput` |
| `binary-file-stream` | `(open-binary name direction)` / `open-binary-input` / `open-binary-output` | `ByteInput` `ByteOutput` |

`direction` is one of the three constants `direction-input` / `direction-output` /
`direction-append`. `open-file` returns `Err(FileError)` if the file cannot be opened (a missing file is
an ordinary result, not a panic). The file name can be a string or a `pathname` (`Pathish` in
chapter 9).

`(get-output-stream-string s)` returns what has been written to a `string-output-stream` and empties
it. As in CL, it can be taken out even after `close`.

**Byte I/O** uses `ByteInput`/`ByteOutput`. These fix the `Item` of `InputStream`/`OutputStream` to
`int`, in the same way `CharInput`/`CharOutput` fix it to `char`.

| Name | Form | Type | Description |
|---|---|---|---|
| `read-byte` | `(read-byte s)` | `(S)→Option<int>` where `ByteInput S` | The next byte. `none` at the end of the file |
| `write-byte` | `(write-byte s b)` | `(S,int)→()` where `ByteOutput S` | Writes one byte. An error outside 0..255 |
| `read-sequence` | `(read-sequence s v n)` | `(S,Vector<int>,int)→int` where `ByteInput S` | The character version, in bytes |
| `write-sequence` | `(write-sequence s v)` | `(S,Vector<int>)→()` where `ByteOutput S` | Same as above |

CL decides the element type in **the call**, as in `(open name :element-type '(unsigned-byte 8))`,
but here the element type is **the type** of the stream, so what differs is the function that opens
it. Reading bytes from a character stream is a type error (`string-input-stream` does not implement
`ByteInput`). Reading a byte right after pushing back a character with `unread-char` is also an error.

## 4. Composite streams

All are `defstruct`s in the standard library and can be nested.

| Name | Form | Description |
|---|---|---|
| `make-broadcast-stream` | `(make-broadcast-stream v)` | Writes to all of a `Vector<:dyn CharOutput>` |
| `make-two-way-stream` | `(make-two-way-stream in out)` | Reads from `in` and writes to `out` |
| `make-echo-stream` | `(make-echo-stream in out)` | Reads from `in` and also writes the characters read to `out` |
| `make-concatenated-stream` | `(make-concatenated-stream v)` | Reads a `Vector<:dyn CharInput>` one after another |
| `make-peek-stream` | `(make-peek-stream in)` | Adds a one-character pushback to any `:dyn CharInput`, making it a `PeekInput` (for `read-sexpr`) |

## 5. Macros

| Name | Form | Description |
|---|---|---|
| `with-open-file` | `(with-open-file (var name direction) body...)` | Open, run the body, close. `Result<value of the body, FileError>` |
| `with-input-from-string` | `(with-input-from-string (var s) body...)` | Reads from a string |
| `with-output-to-string` | `(with-output-to-string (var) body...)` | Returns what was written |

## 6. Generic functions and file operations

| Name | Form | Type | Description |
|---|---|---|---|
| `copy-stream` | `(copy-stream from to)` | `(I,O)→()` where `CharInput I`,`CharOutput O` | Transfers everything |
| `read-lines` | `(read-lines s)` | `(S)→Vector<string>` where `CharInput S` | All the remaining lines |
| `read-sexpr` | `(read-sexpr s)` | `(S)→Result<ReadOutcome,ReadError>` where `PeekInput S` | Reads one `Sexpr` (CL's `read`). `Ok(eof)` at the end of input, `Ok(datum d)` when one is read, `Err` if it is not data. It **consumes the one whitespace character** that ended the datum (as in CL). `ReadOutcome` is not an `Option<Sexpr>` so that reading the empty list `()` and the end of input are not the same value |
| `read-sexpr-preserving-whitespace` | Same as above | Same as above | The same, but leaves the whitespace (CL's `read-preserving-whitespace`) |
| `read-delimited-list` | `(read-delimited-list ch s)` | `(char,S)→Result<Option<Sexpr>,ReadError>` where `PeekInput S` | Reads up to `ch` and makes a list. `ch` is consumed. `Err` if the input runs out |
| `write-lines` | `(write-lines s lines)` | `(S,I)→()` where `CharOutput S`,`Iter I (Item string)` | Writes one line at a time |
| `read-file-string` | `(read-file-string name)` | `(P)→Result<string,FileError>` where `Pathish P` | The whole contents |
| `read-file-lines` | `(read-file-lines name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | All the lines |
| `write-file-string` | `(write-file-string name text)` | `(P,string)→Result<(),FileError>` where `Pathish P` | Writes it out |
| `probe-file` | `(probe-file name)` | `(P)→bool` where `Pathish P` | Whether it exists |
| `delete-file` / `rename-file` | | `→Result<(),FileError>` | Delete, rename (arguments are `Pathish`) |
| `truename` | `(truename name)` | `(P)→Result<string,FileError>` where `Pathish P` | The absolute path with symbolic links and `.`/`..` resolved. `Err` if it does not exist |
| `file-write-date` | `(file-write-date name)` | `(P)→Result<universal-time,FileError>` where `Pathish P` | The time of the last modification. It is **universal time**, so `decode-universal-time` ([Time](system.md#2-decoding-and-encoding-dates)) can read it |
| `file-author` | `(file-author name)` | `(P)→Result<Option<string>,FileError>` where `Pathish P` | The owner's login name. `Err` if the file does not exist, `Ok(none)` if the owner's uid has no entry in the password database: the two cases CL distinguishes are kept apart |
| `directory-p` | `(directory-p name)` | `(P)→bool` where `Pathish P` | Whether it is a directory. **Also `false` if it does not exist**; use `probe-file` to tell the two apart |
| `directory` | `(directory name)` | `(P)→Result<Vector<string>,FileError>` where `Pathish P` | Lists the contents by truename (the absolute path with symbolic links resolved, as with `truename`). Symbolic links whose target is missing are left out. `.`/`..` are left out. The order is whatever the OS gives |
| `ensure-directories-exist` | `(ensure-directories-exist name)` | `(P)→Result<(),FileError>` where `Pathish P` | Creates it along with its parents. Succeeds if it already exists |

Every argument naming a file **can be a string or a `pathname`**. This is the same treatment as CL's
pathname designators, resolved through the `Pathish` trait rather than a run-time type test
(chapter 9).

The terminating character of `read-delimited-list` **also ends tokens**. It takes effect only at
depth 0: in `(1 2]` the `]` is read as part of the list's own text and reported as a broken list.
There is no counterpart to CL's third argument `recursive-p`.

## 7. Making your own type a stream

Write one `write-item` and the default implementations bring along the rest. It can also go into
composite streams.

```lisp
(defstruct counter (n i32))
(impl Stream counter
  (open-stream-p ((self Self)) bool true)
  (close ((self Self)) () ()))
(impl OutputStream counter
  (type Item char)
  (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
(impl CharOutput counter)              ; every remaining method is the default

(write-line (counter::new 0) "four")   ; write-line, terpri and fresh-line all work
```

Input works the same way: you only write `read-item`. Even a type with no pushback of its own can be
`read` once wrapped, as in `(read-sexpr (make-peek-stream my-stream))`.

## 8. readtable

| Name | Call | Type | Description |
|---|---|---|---|
| `set-macro-character` | `(set-macro-character c f)` | `(char, F)→()` | `f` reads the character `c` |
| `get-macro-character` | `(get-macro-character c)` | `(char)→Option<F>` | Returns what is registered |
| `set-dispatch-macro-character` | `(set-dispatch-macro-character d s f)` | `(char,char,F)→()` | `f` reads the two-character sequence `d s` |
| `get-dispatch-macro-character` | `(get-dispatch-macro-character d s)` | `(char,char)→Option<F>` | Same as above |

`F` is `(fn (string-input-stream char) Option<Sexpr>)`. How to use them, when they take effect and how
they differ from CL are in the [Syntax Reference](../syntax.md#11-reader-macros-readtable).

## 9. Pathnames `pathname`

A file name split into parts. It holds the `/`-separated directory components, the name, the type
(extension), and whether it starts at the root.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")   split at the last dot
  (pathname-type p)        ; => (some "gz")
  (namestring p))          ; => "/var/log/app.tar.gz"

(namestring (merge-pathnames (make-pathname :name "today" :type "log")
                             "/var/log/"))        ; => "/var/log/today.log"
```

### 9.1 The pathname designator trait `Pathish`

Where CL accepts a pathname designator (a string or a pathname), this language accepts a `Pathish`.
Both `string` and `pathname` implement it, and **every file operation takes it generically**, so
`(open-input "a.txt")` and `(open-input p)` are both ordinary calls (there is no run-time type test).
The `namestring` of a string just returns itself, so as long as you pass a string, no parsing happens.

| Name | Form | Type | Description |
|---|---|---|---|
| `namestring` | `(namestring p)` | `(P)→string` | The string form. Must be implemented |
| `to-pathname` | `(to-pathname p)` | `(P)→pathname` | Converts to a `pathname` (CL's `pathname` function, renamed because it would clash with the type name). Must be implemented |

### 9.2 Functions

| Name | Form | Type | Description |
|---|---|---|---|
| `parse-namestring` | `(parse-namestring s)` | `string→pathname` | Splits a string into parts. A trailing `/` (or an empty name) means "no name", that is, a directory |
| `make-pathname` | `(make-pathname :directory v :name s :type s :absolute b)` | `→pathname` | Builds one from just the components given (all `&key`). A name or type left out stays "absent" and is something `merge-pathnames` fills in |
| `pathname-directory` | `(pathname-directory p)` | `(P)→Vector<string>` | The directory components, outermost first |
| `pathname-name` | `(pathname-name p)` | `(P)→Option<string>` | The name without the type. `none` for a directory |
| `pathname-type` | `(pathname-type p)` | `(P)→Option<string>` | After the last dot. A leading dot does not count (all of `.gitignore` is the name) |
| `pathname-absolute-p` | `(pathname-absolute-p p)` | `(P)→bool` | Whether it starts at the root |
| `user-homedir-pathname` | `(user-homedir-pathname)` | `()→Option<pathname>` | The home directory. `none` if there is no `$HOME` (CL allows `NIL` too) |
| `directory-namestring` | `(directory-namestring p)` | `(P)→string` | The part up to the last `/` |
| `file-namestring` | `(file-namestring p)` | `(P)→string` | Just the `name.type` part |
| `merge-pathnames` | `(merge-pathnames p default)` | `(P,D)→pathname` | Fills the components missing from `p` from `default`. A relative `p` goes under the directory of `default`; an absolute `p` keeps its own directory |
| `enough-namestring` | `(enough-namestring p default)` | `(P,D)→string` | The form relative to `default`. All of `p` if it is not under the base |

The type arguments all carry `(where (Pathish P))`.

## 10. Differences from CL

- **A trait hierarchy, not a class hierarchy.** There is no `input-stream-p` / `output-stream-p`: the
  type carries the direction, so it is not a question to ask at run time.
- **`read` has different names for the string and stream versions.** `(read "...")` (corresponds to
  the first value of CL's `read-from-string`; if you also need the position where reading ended, use
  `read-from-string`) and `(read-sexpr s)` (CL's `read`). A call resolves to one receiver type, so the
  same name cannot be overloaded.
- **Pushback is a separate trait** (`PeekInput`), so types that only need `read-char` are not forced to
  implement `unread-char`.
- **Closing is explicit.** The GC does not close streams (the GC runs at unpredictable times, so
  leaving it to the GC would make the moment of closing unpredictable too). Using `with-open-file` is
  the safe way.
- **Pathnames have no host, device or version components.** There are no wildcard pathnames and no
  logical pathnames (`logical-pathname`). The separator is always `/`.
- **The `pathname` function is `to-pathname`**, because types, traits and functions share one
  namespace.
- **There is no matching by wildcards**, so `directory` is a function that "lists the contents of that
  directory" and nothing more. CL's `directory` matches against a pathname pattern.
