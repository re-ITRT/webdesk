@echo off
rem dsh-web.bat - 启动 dsh web（后台新窗口，常驻不被回收）
start "dsh-web" cmd /k wsl -d Ubuntu-24.04 -e bash -lc "export PATH=/home/postel/.nvm/versions/node/v24.19.0/bin:/usr/bin:/bin; cd ~; exec dsh --profile web --no-open"
echo dsh web 已在后台启动
