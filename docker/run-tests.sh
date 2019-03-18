docker-compose up -d 
for idx in $(seq 1 4); do docker-compose exec --index=$idx prov gu-provider hubs connect 172.19.0.2:61622; done
docker-compose exec hub /bin/bash

