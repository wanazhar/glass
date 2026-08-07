#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"ok":true,"result":{"extension":"intent-evidence","evidence":["role","name","visibility"]}}'
