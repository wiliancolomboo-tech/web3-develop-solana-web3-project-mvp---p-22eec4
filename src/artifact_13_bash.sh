# Check container status
docker inspect --format='{{json .State.Health}}' $(docker-compose ps -q univibe-worker)