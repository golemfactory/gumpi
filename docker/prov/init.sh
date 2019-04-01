#! /bin/bash

service ssh start

su -c "/usr/bin/gu-provider server run" $USER
