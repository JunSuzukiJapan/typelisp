<!-- translated-from: docs/ja/reference/functions/network.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Rede (TCP / TLS / domínio Unix / UDP)

Os sockets fazem parte dos [streams](streams-files.md). Uma conexão é vista em **duas visões da mesma
conexão**, `socket-stream` (caracteres) e `socket-byte-stream` (bytes), e `read-line`/`write-line`/
`read-byte`/`format` funcionam sobre elas como estão. Um listener é um `socket-listener`. **TCP, TLS e sockets
de domínio Unix compartilham um tipo** (como o `net.Conn` do Go): depois de conectados, ler e escrever é igual,
e só muda a forma como são criados. UDP não é um stream, mas datagramas (`udp-socket`).

**O que espera é a tarefa, não a thread.** `accept`, `read-line`, `write-string`, `tcp-connect` (incluindo a
resolução de nomes) e `recv-from` param *aquela tarefa* se não estiverem prontos (como `sleep`/`recv`), e as
outras tarefas continuam em execução. É por isso que um servidor pode ser escrito na mesma forma que no Go,
`(task (serve c))` por conexão ([Referência de sintaxe 12.5](../syntax.md#125-onde-as-tarefas-alternam)).

## 1. Tipos

| Tipo | Traits implementados | Como obter um |
|---|---|---|
| `socket-stream` | `CharInput` `PeekInput` `CharOutput` | `tcp-connect` / `tls-connect` / `unix-connect` / `accept` / `char-stream-of` |
| `socket-byte-stream` | `ByteInput` `ByteOutput` | `byte-stream-of` |
| `socket-listener` | `Stream` (`close` / `open-stream-p`) | `tcp-listen` / `tls-listen` / `unix-listen` |
| `udp-socket` | `Stream` | `udp-bind` |
| `datagram` | — (`bytes`: `Vector<int>`, `from`: `string`) | `recv-from` |
| `NetError` | `Error` | O `Err` das funções abaixo |

Um tipo de stream tem um único tipo de elemento (pelo mesmo motivo de `file-stream`/`binary-file-stream`), então
caracteres e bytes são tipos diferentes. `byte-stream-of`/`char-stream-of` devolvem valores que apontam para
**a mesma conexão** e compartilham o buffer de recepção: é assim que se escrevem coisas como HTTP, em que os
cabeçalhos são lidos como caracteres e o corpo como bytes. Ler um byte logo depois de um `unread-char` de um
caractere é um erro (a mesma regra dos arquivos).

## 2. TCP / TLS / domínio Unix

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `tcp-connect` | `(tcp-connect host port &key timeout)` | `(string,int,&key f64)→Result<socket-stream,NetError>` | Conecta. `host` pode ser um nome ou um endereço. Se um nome tiver vários endereços, eles são tentados em sequência (`localhost` é `::1` e `127.0.0.1`). Uma busca de nome que falha, uma conexão recusada ou ultrapassar `:timeout` segundos dão `Err` |
| `tls-connect` | `(tls-connect host port &key timeout ca-file cert-file key-file server-name)` | `(string,int,&key f64 string string string string)→Result<socket-stream,NetError>` | TLS depois de `tcp-connect`. O certificado é verificado contra o nome `host` (`:server-name` se o nome a verificar diferir de onde você conecta) com os certificados raiz da Mozilla. Com `:ca-file` (PEM), confia **só nos certificados que ele contém** (uma CA privada, ou o próprio certificado que seu `tls-listen` apresenta). `:cert-file`/`:key-file` (ambos ou nenhum) são o nosso certificado, apresentado quando o servidor pede um (TLS mútuo). O handshake é concluído aqui, então se o certificado do servidor não passar, esta chamada devolve `Err`. O resultado é um `socket-stream` comum |
| `unix-connect` | `(unix-connect path)` | `(string)→Result<socket-stream,NetError>` | Conecta ao socket de domínio Unix `path`. É local, então não há espera de handshake nem tempo limite |
| `tcp-listen` | `(tcp-listen host port)` | `(string,int)→Result<socket-listener,NetError>` | Escuta. `"127.0.0.1"` é só esta máquina, `"0.0.0.0"` é todas as interfaces. Passar `0` como `port` deixa o SO escolher |
| `tls-listen` | `(tls-listen host port cert-file key-file &key client-ca)` | `(string,int,string,string,&key string)→Result<socket-listener,NetError>` | A versão TLS de `tcp-listen`. `cert-file` é a cadeia de certificados (PEM, o próprio certificado primeiro), e `key-file` a chave privada. Ambos são lidos e verificados aqui, então uma chave que não corresponde dá `Err` nesta chamada, não no primeiro cliente. `accept` retorna **antes do handshake**, e a primeira leitura ou escrita da tarefa que atende a conexão conclui o handshake (como o `tls.Conn` do Go), então um cliente lento no handshake não segura outros `accept`. Com `:client-ca` (PEM), **exige de todo cliente** um certificado emitido por uma CA que ele contém (TLS mútuo). Sem ele, nenhum é pedido |
| `unix-listen` | `(unix-listen path)` | `(string)→Result<socket-listener,NetError>` | Escuta em `path`. **`Err` se o arquivo já existir** (ele pode pertencer a outro processo em execução, então não é substituído silenciosamente). `close` remove o arquivo |
| `accept` | `(accept l &key timeout)` | `(socket-listener,&key f64)→Result<socket-stream,NetError>` | A próxima conexão. Para a tarefa até chegar uma. Desiste com `Err` depois de `:timeout` segundos |
| `wait-readable` | `(wait-readable s secs)` | `(socket-stream,f64)→bool` | Até que possa ser lido sem esperar, ou `secs` segundos. `true` significa o primeiro caso (incluindo dados já no buffer). A forma de pôr um relógio em uma leitura: `(if (wait-readable c 5.0) (read-line c) ...)`. O que ele promete é que **a próxima leitura não para**; `read-line` pode esperar pelo resto da linha |
| `wait-writable` | `(wait-writable s secs)` | `(socket-stream,f64)→bool` | Até que possa ser escrito, ou `secs` segundos |
| `tls-add-certificate` | `(tls-add-certificate l name cert-file key-file)` | `(socket-listener,string,string,string)→Result<(),NetError>` | Adiciona mais um certificado a um listener de `tls-listen`, apresentado aos clientes que pedem `name` (SNI): vários sites em um listener. Aqui se verifica se a cadeia é para `name`; se não for, esta chamada devolve `Err`. Clientes que pedem um nome que ninguém adicionou, ou nenhum nome, recebem o certificado de `tls-listen` |
| `peer-subject` | `(peer-subject s)` | `(socket-stream)→Option<string>` | O sujeito do certificado do outro lado (`CN=client,O=Example,C=JP`, forma RFC 4514, o mais específico primeiro). Como um servidor com TLS mútuo descobre "quem se conectou" (depois da primeira leitura, que conclui o handshake). No lado do cliente, o nome do certificado do servidor. `none` para conexões em texto puro, antes do handshake, ou se o outro lado não apresentou certificado |
| `requested-server-name` | `(requested-server-name s)` | `(socket-stream)→Option<string>` | Em uma conexão TLS do lado do servidor, o nome que o cliente pediu (SNI). Como um servidor com vários sites adicionados por `tls-add-certificate` descobre "qual site". `none` para conexões em texto puro, no lado do cliente ou sem nome (conectado por endereço) |
| `set-nodelay` | `(set-nodelay s on)` | `(socket-stream,bool)→()` | Desliga o Nagle (`TCP_NODELAY`). `write-string` vai até o socket toda vez, então com o Nagle ligado, uma resposta escrita em duas partes, cabeçalho e corpo, espera o ACK atrasado do outro lado: use `true` para protocolos de requisição/resposta. Sockets de domínio Unix não têm Nagle, e simplesmente dá certo |
| `set-keepalive` | `(set-keepalive s on)` | `(socket-stream,bool)→()` | Sonda o outro lado enquanto está ocioso (`SO_KEEPALIVE`). Detecta um outro lado que sumiu sem fechar (um cabo desconectado, um host parado) e reinicia a conexão. O período padrão do SO é longo (muitas vezes 2 horas), então combine-o com `set-keepalive-period` abaixo. Sockets de domínio Unix não têm isso, então ocorre panic |
| `set-keepalive-period` | `(set-keepalive-period s secs)` | `(socket-stream,int)→()` | Os segundos de ociosidade antes da primeira sonda e o intervalo entre sondas (segundos inteiros, pelo menos 1). Como o `SetKeepAlivePeriod` do Go, ambos recebem o mesmo valor |
| `socket-error` | `(socket-error s)` | `(socket-stream \| socket-byte-stream)→Option<NetError>` | Se **o outro lado** quebrou esta conexão, sua primeira falha (um reinício, um alerta TLS, uma desconexão durante uma escrita). `none` se estiver saudável: um EOF do outro lado fechando de forma limpa não é uma falha. Veja "Falhas do outro lado" abaixo |
| `local-address` | `(local-address s)` | `(socket-stream \| socket-listener \| udp-socket)→string` | O `host:port` do nosso lado. A forma de saber a porta escolhida depois de `(tcp-listen h 0)`. Para sockets Unix, o caminho (o lado que conecta é `(unnamed)`) |
| `peer-address` | `(peer-address s)` | `(socket-stream)→string` | O `host:port` do outro lado, ou o caminho para sockets Unix |
| `shutdown-output` | `(shutdown-output s)` | `(socket-stream)→()` | Fecha só o lado de envio (um half-close). O outro lado lê EOF, e este lado ainda pode ler. O sinal de "enviei a requisição inteira". No TLS também envia `close_notify` |
| `byte-stream-of` | `(byte-stream-of s)` | `(socket-stream)→socket-byte-stream` | A versão em bytes da mesma conexão |
| `char-stream-of` | `(char-stream-of b)` | `(socket-byte-stream)→socket-stream` | A versão em caracteres da mesma conexão |
| `close` | `(close s)` | `Stream` | Esvazia o buffer e então fecha. O GC não a fecha |
| `with-connection` | `(with-connection (var host port) body...)` | Macro | Conectar, executar o corpo, fechar. `Result<valor do corpo, NetError>` (a mesma forma de `with-open-file`) |

`write-string`/`write-line` **retornam depois que tudo foi escrito** (como o `net.Conn.Write` do Go). Para
agrupar escritas pequenas, acumule-as em um `string-output-stream` e escreva uma vez. `listen` é `true` só
quando há algo no buffer de recepção, então `read-char-no-hang` funciona como o nome diz.

**Falhas do outro lado não causam panic.** Se o outro lado reiniciar a conexão, rejeitar o handshake TLS ou
cortá-la no meio de uma escrita, isso não é um erro deste programa, então o servidor não para junto com seus
outros clientes. A primeira falha é registrada na conexão; as leituras seguintes devolvem `none` (com a mesma
aparência de um EOF), e as escritas não vão a lugar nenhum e retornam silenciosamente. Para distingui-los, use
`socket-error`, com a mesma forma do `bufio.Scanner.Err` do Go (`read-item` é um `Option` e não tem outro canal
por onde relatar). Só os erros do próprio programa causam panic (um handle fechado, ler um byte logo depois de
`unread-char`).

**Os tempos limite são argumentos explícitos ou `wait-readable`.** Não há uma forma, como o
`SetReadDeadline` do Go, em que um stream carrega um prazo e `read-line` falha: `read-item` devolve
`Option<Item>` e não tem como devolver um erro. Um relógio é necessário em três lugares, ao conectar, ao aceitar
e na "próxima leitura", e cada um tem um argumento para isso.

```lisp
;; servidor: uma tarefa por conexão
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c line))
          ((none) (break))))
  (close c))

(let ((l (unwrap (tcp-listen "0.0.0.0" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))

;; cliente
(match (with-connection (c "127.0.0.1" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "~a" reply))
  ((err e) (println "~a" (message e))))

;; HTTPS
(let ((c (unwrap (tls-connect "example.com" 443 :timeout 10.0))))
  (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
  (println "~a" (unwrap (read-line c)))       ; HTTP/1.1 200 OK
  (close c))

;; servidor TLS (o certificado pode ser criado, por exemplo, com
;;   openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost \
;;           -addext subjectAltName=DNS:localhost -keyout key.pem -out cert.pem)
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))        ; o serve acima como está; o primeiro read-line é o handshake
          ((err e) (println "accept: ~a" (message e))))))
;; seu cliente: conectar confiando no próprio certificado
(unwrap (tls-connect "localhost" 8443 :timeout 5.0 :ca-file "cert.pem"))

;; TLS mútuo: o servidor exige um certificado de cliente emitido por ca.pem, e o cliente apresenta um
(tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem" :client-ca "ca.pem")
(tls-connect "localhost" 8443 :ca-file "ca.pem" :cert-file "client.pem" :key-file "client-key.pem")
;; no lado do servidor, depois do primeiro read-line: quem se conectou
(println "~a" (unwrap-or (peer-subject c) "anonymous"))    ; CN=client,O=Example

;; dois sites em um listener (SNI)
(let ((l (unwrap (tls-listen "0.0.0.0" 443 "a.example.pem" "a.key"))))
  (unwrap (tls-add-certificate l "b.example" "b.example.pem" "b.key"))
  ...)                                        ; na conexão, (requested-server-name c) diz qual
```

Com TLS mútuo, um cliente que não apresenta certificado (ou apresenta um que não passa) conclui o seu lado do
handshake antes de o servidor decidir, no TLS 1.3, então `tls-connect` devolve `Ok` e **a primeira leitura
devolve `none`** (com o alerta em `socket-error`). No lado do servidor, a primeira leitura da mesma conexão
também devolve `none`. Nenhum dos lados entra em panic.

## 3. UDP

| Nome | Uso | Tipo | Significado |
|---|---|---|---|
| `udp-bind` | `(udp-bind host port)` | `(string,int)→Result<udp-socket,NetError>` | Cria um socket. Necessário até só para enviar (`port` é `0`) |
| `send-to` | `(send-to s host port bytes)` | `(udp-socket,string,int,Vector<int>)→Result<(),NetError>` | Envia um datagrama. Os nomes são resolvidos. Não se sabe se ele chegou (UDP) |
| `recv-from` | `(recv-from s &key timeout)` | `(udp-socket,&key f64)→Result<datagram,NetError>` | O próximo datagrama. `from` é `ip:port` e pode ser passado como está como o `host` de `send-to` |

Converta entre strings e sequências de bytes com `string->utf8` / `utf8->string`
([Strings](collections.md#1-strings-string)).

```lisp
(let ((s (unwrap (udp-bind "127.0.0.1" 0))))
  (unwrap (send-to s "127.0.0.1" 9999 (string->utf8 "ping")))
  (match (recv-from s :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
```

## 4. O que não existe

- Uma forma de ler do certificado do outro lado **algo além do sujeito** (SAN, período de validade, emissor).
- **Esperar ao mesmo tempo um socket e um canal com `select`.** Como no Go, escreva como "iniciar uma tarefa
  que lê e fazê-la alimentar um canal".
- **HTTP/2**. **Datagramas** de domínio Unix (`SOCK_DGRAM`).
- **Prazos carregados por um stream** (pelos motivos acima, os relógios são argumentos).
