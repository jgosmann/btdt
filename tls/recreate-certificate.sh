#!/usr/bin/env bash

set -o errexit -o nounset -o pipefail -o xtrace

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

touch index.txt
echo 1000 > serial
openssl req -new -x509 \
  -days 3650 \
  -nodes \
  -keyout ca.key \
  -out ca.pem \
  -config ca.cnf \
  -extensions v3_ca \
  -subj "/O=btdt/CN=btdt CA"
openssl req -new -newkey rsa:2048 -nodes -keyout leaf.key -out leaf.csr -subj "/O=btdt/CN=localhost"
openssl ca -batch -config ca.cnf -extensions leaf_cert -extfile openssl.cnf -in leaf.csr -out leaf.pem -days 3650
openssl pkcs12 -export -inkey leaf.key -in leaf.pem -out leaf.p12 -passout pass:password

rm 1000.pem index.txt index.txt.attr index.txt.old