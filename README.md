# mini-docker
This project aims at creating a toy docker project for third year in school

## Prerequisites
Since we need to have linux env for this project, we need to use docker in order to create such environment

```shell
docker run --name mydocker --it --CAP-ADD=SYS_ADMIN -it ubuntu sh 
``` 
You can simply stop the container by typing `exit` and re run it by running :  
```shell
docker exec mydocker -it sh
```

