# Authorization

Authorization is done with [Eclipse Biscuit](https://www.biscuitsec.org/) tokens.
This avoids the need to manage user accounts on the server.
The server only needs to have a private key to verify the tokens.

To manage private keys and authorization tokens, use the `biscuit` command line tool that can be
installed with

```sh
cargo install biscuit-cli
```

## Private authorization key

The server uses a private key to verify the validity of authorization tokens.

The location of the private key can be configured via the `auth_private_key` configuration option
(or the `BTDT_AUTH_PRIVATE_KEY` environment variable). Note that the private key's permission must be restricted to
`0600`.

If the private key is not present at server startup, a new key will be generated.

To manually generate a new private key, use

```sh
biscuit keypair --key-output-format pem --only-private-key | head -c -1 > auth_private_key.pem
```

(The `biscuit` tool outputs a trailing newline that prevents reading it to generate tokens, so we remove it with
`head -c -1`.)

## Generating authorization tokens

To generate a new authorization token with all permissions and validity of 90 days, use

```sh
biscuit generate \
  --private-key-file auth_private_key.pem \
  --private-key-format pem \
  --add-ttl 90d - <<EOF
EOF
```

This will output a new authorization token that can be used with `btdt` to access the server.

It is also possible to restrict the permissions of the token by adding
additional [Datalog](https://doc.biscuitsec.org/reference/datalog.html) statements.
For this, the following facts can be used:

- `cache($cache_id)` declares the cache that is being accessed.
- `operation($op)` declares the operation being performed. Valid operations are `get` and `put`.

For example, to generate a token that only allows reading from the cache `my-cache`, use

```sh
biscuit generate \
  --private-key-file auth_private_key.pem \
  --private-key-format pem \
  --add-ttl 90d - <<EOF
  check if operation("get");
  check if cache("my-cache");
EOF
```

## Attenuating authorization tokens

Authorization tokens can be attenuated to further restrict their permissions or validity period.
To attenuate a token, use the `biscuit attenuate` command.
For example, to attenuate an existing token to only allow accessing the cache `my-cache` for another 30 days, use

```sh
biscuit attenuate \
    --block 'check if cache("my-cache");' \
    --add-ttl 30d \
    file-with-token
```

## Revoking authorization tokens

Revocation is not yet implemented. If you have to revoke a token, you will have to rotate the private key and issue
new tokens.
