# Throwaway OpenSSH target for Wormhole integration tests. Never expose publicly.
FROM alpine:3.20
RUN apk add --no-cache openssh socat bash \
 && adduser -D -s /bin/bash wh \
 && echo 'wh:whpass' | chpasswd \
 && ssh-keygen -A
COPY --from=keys authorized_keys /home/wh/.ssh/authorized_keys
COPY sshd_config /etc/ssh/sshd_config
RUN chown -R wh:wh /home/wh/.ssh && chmod 700 /home/wh/.ssh && chmod 600 /home/wh/.ssh/authorized_keys
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
