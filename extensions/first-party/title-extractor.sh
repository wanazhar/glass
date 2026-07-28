#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"ok":true,"result":{"extension":"title-extractor","fields":["title"]}}'
