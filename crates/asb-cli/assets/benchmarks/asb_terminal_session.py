"""SDK-shell launch transport; not enabled by the production task dispatcher yet.

The parent SDK shell survives a foreground launcher and failed coding client.
Private FIFO IO is bridged separately; only the stopped child can be measured.
"""
import json
import re
import shlex
import threading
import time
import uuid

from asb_execution import DockerExecTransport, ExecutionError


def _publish_receipt(root, name, nonce, *variables):
    """Expose only a fully written receipt; never replace an existing receipt."""
    q = shlex.quote
    temporary, final = q(root+"/"+name+".tmp"), q(root+"/"+name)
    fields = " ".join([q(nonce)]+['"$'+value+'"' for value in variables])
    return ("(umask 077; set -C; printf "+q(" ".join(["%s"]*(1+len(variables)))+"\\n")+
        " "+fields+" >"+temporary+") && ln -T -- "+temporary+" "+final)


class SDKSessionLauncher:
    def __init__(self, session, host_binary, artifacts, *, require_host_target=True):
        self.session,self.host_binary,self.artifacts = session,str(host_binary),artifacts
        self.require_host_target,self.transports = require_host_target,[]

    def __call__(self,api,container,command,environment,workdir):
        transport = SessionTransport(self,api,container)
        self.transports.append(transport)  # Own even a partially failed start.
        try:transport.start(command,environment,workdir)
        except BaseException as error:
            try:transport.close()
            except BaseException:
                raise ExecutionError("SDK session launch failed and transport settlement remains unconfirmed") from error
            raise
        return transport

    def close(self):
        failed = False
        for transport in reversed(self.transports):
            try:transport.close()
            except BaseException:failed = True
        if failed:raise ExecutionError("SDK session launch transport settlement remains unconfirmed")


class SessionTransport:
    def __init__(self,owner,api,container):
        self.owner,self.api,self.container = owner,api,container
        self.nonce = uuid.uuid4().hex
        self.root = "/tmp/asb-session-"+self.nonce
        self.channel = "asb-return-"+self.nonce
        self.bridge,self.worker,self.worker_error = None,None,None
        self.namespace_pid,self.parent_pid,self.closed,self.close_error = None,None,False,None
        self.deadline = time.monotonic()+90

    @property
    def exec_id(self):
        if self.bridge is None:raise ExecutionError("SDK session bridge has not started")
        return self.bridge.exec_id

    def _run(self,command):
        return self.container.exec_run(command,user=self.owner.session._user)

    def start(self,command,environment,workdir):
        from asb_guest import upload
        session = self.owner.session
        self.container.reload()
        labels = self.container.attrs.get("Config",{}).get("Labels",{}) or {}
        if (session.container.id != self.container.id or labels.get("asb.owner") != "asb"
                or session._user != self.container.attrs.get("Config",{}).get("User","")
                or not session.is_session_alive()):
            raise ExecutionError("SDK session is not the configured-user shell of this owned container")
        identities = []
        for flag in ["-u","-g"]:
            result = self._run(["id",flag])
            if result.exit_code != 0 or not re.fullmatch(rb"[0-9]{1,10}\n",result.output):
                raise ExecutionError("cannot identify configured SDK session user")
            identities.append(int(result.output))
        result = self._run(["/bin/sh","-c",'umask 077; mkdir "$1" && mkfifo "$1/out" "$1/err" "$1/control"',
            "asb-session",self.root])
        if result.exit_code != 0:raise ExecutionError("cannot create private SDK launch channels")
        q = shlex.quote
        exports = "\n".join("export "+key+"="+q(value) for key,value in environment.items())
        # A noninteractive foreground launcher waits for the STOPped application
        # without interactive job-control returning the SDK parent to a prompt.
        script = ("#!/bin/bash\nset +x\nset +e\nset +m\n(\n"+exports+"\nexec "+shlex.join(command)+
            "\n) >"+q(self.root+"/out")+" 2>"+q(self.root+"/err")+" < /dev/null &\n"
            "asb_child_pid=$!\n"
            +_publish_receipt(self.root,"started",self.nonce,"asb_child_pid")+
            " || exit 1\nif wait \"$asb_child_pid\"; then asb_child_status=0; else asb_child_status=$?; fi\n"
            +_publish_receipt(self.root,"exit",self.nonce,"asb_child_pid","asb_child_status")+" || exit 1\n"
            "printf '%s\\n' "+q(self.nonce)+" >"+q(self.root+"/control")+"\nexit 0\n")
        if len(script.encode()) > 2*1024**2:raise ExecutionError("SDK session launcher exceeds its bound")
        upload(self.container,self.root,[("launch.sh",script.encode(),0o600)],
            temporary_dir=self.owner.artifacts,owner=tuple(identities))
        keys = [_publish_receipt(self.root,"parent",self.nonce,"$")+
            " && { bash "+q(self.root+"/launch.sh")+"; "+
            _publish_receipt(self.root,"returned",self.nonce,"$")+" && tmux wait -S "+q(self.channel)+"; }","Enter"]
        def launch():
            try:session.send_keys(keys,block=False,min_timeout_sec=0)
            except BaseException as error:
                self.worker_error = type(error).__name__
                try:self.container.kill()
                except Exception:pass
        self.worker = threading.Thread(target=launch,daemon=True)
        self.worker.start()
        bridge = ('cat "$1/out" & asb_out=$!; cat "$1/err" >&2 & asb_err=$!; '
            'wait "$asb_out" || exit 1; wait "$asb_err" || exit 1; '
            'dd bs=129 count=1 of=/dev/null 2>/dev/null < "$1/control"')
        self.bridge = DockerExecTransport(self.api,self.container,
            ["/bin/sh","-c",bridge,"asb-session-bridge",self.root],{},workdir)

    def __iter__(self):return self
    def __next__(self):return next(self.bridge)

    def _frame(self,name,count,deadline=None):
        deadline = self.deadline if deadline is None else deadline
        while time.monotonic() < deadline:
            result = self._run(["/bin/sh","-c",'test -f "$1" && test ! -L "$1" && head -c 129 "$1"',
                "asb-session-receipt",self.root+"/"+name])
            if result.exit_code == 0:
                fields = result.output.split()
                if (len(result.output) > 128 or not result.output.endswith(b"\n") or len(fields) != count
                        or fields[0] != self.nonce.encode()
                        or not all(re.fullmatch(rb"[0-9]{1,10}",field) for field in fields[1:])):
                    raise ExecutionError("SDK session returned a malformed private receipt")
                return [int(field) for field in fields[1:]]
            if self.worker_error:raise ExecutionError("SDK session launch worker failed")
            time.sleep(.02)
        raise ExecutionError("SDK session receipt did not arrive before its preparation deadline")

    def target(self,namespace_pid):
        from asb_material_exec import run
        from asb_material import MaterialError
        if self._frame("started",2) != [namespace_pid]:
            raise ExecutionError("SDK session gate does not match its launched child")
        self.worker.join(timeout=max(0,min(5,self.deadline-time.monotonic())))
        if self.worker.is_alive() or self.worker_error:
            raise ExecutionError("SDK session launch worker did not settle before client admission")
        self.parent_pid, = self._frame("parent",2)
        if self.parent_pid <= 1 or self.parent_pid == namespace_pid:
            raise ExecutionError("SDK session client replaced its parent shell")
        self.namespace_pid = namespace_pid
        probe = self._run(["/opt/asb-runtime/asb","__asb-target",str(namespace_pid)])
        if probe.exit_code != 0 or len(probe.output) > 4096:
            raise ExecutionError("SDK session child has no bounded process identity")
        try:identity = json.loads(probe.output)
        except (ValueError,UnicodeError):raise ExecutionError("SDK session child identity is malformed") from None
        base = {"container_id":self.container.id,"exec_id":self.exec_id,
            "namespace_pid":namespace_pid,"process_identity":identity}
        try:
            top = self.container.top(ps_args="-eo pid")
            rows = top["Processes"]
            if top["Titles"] != ["PID"] or not isinstance(rows,list) or not 1 <= len(rows) <= 4096:
                raise ExecutionError("SDK session process candidates exceed bounds")
            if any(not isinstance(row,list) or len(row) != 1 or not isinstance(row[0],str)
                    or not re.fullmatch(r"[0-9]{1,10}",row[0]) for row in rows):
                raise ExecutionError("SDK session process candidates are malformed")
            request = dict(base,candidates=[int(row[0]) for row in rows])
            resolved = run([self.owner.host_binary,"__asb-target","--resolve"],json.dumps(request).encode(),
                deadline=min(self.deadline,time.monotonic()+5),output_limit=4096)
            return json.loads(resolved)
        except (MaterialError,ExecutionError,KeyError,ValueError,TypeError):
            if self.owner.require_host_target:raise ExecutionError("SDK session actual child has no verified local host PID") from None
            return dict(base,daemon_pid=None,pid_scope="container-namespace")

    def exit_status(self):
        if self.bridge.exit_status() != 0:raise ExecutionError("SDK session output bridge failed")
        pid,status = self._frame("exit",3,time.monotonic()+5)
        if pid != self.namespace_pid or not 0 <= status <= 255:
            raise ExecutionError("SDK session child status contradicts its stopped gate")
        if not self.owner.session.is_session_alive():raise ExecutionError("SDK parent shell did not survive the client")
        returned = self._run(["timeout","5s","tmux","wait",self.channel])
        if returned.exit_code != 0:raise ExecutionError("SDK parent shell did not confirm foreground command return")
        if self._frame("returned",2,time.monotonic()+5) != [self.parent_pid]:
            raise ExecutionError("SDK parent shell identity changed during client execution")
        return status

    def close(self):
        if self.close_error:raise ExecutionError("SDK session transport closure remains unconfirmed")
        if self.closed:return
        self.closed = True
        try:
            try:
                if self.bridge is not None:self.bridge.close()
            finally:
                if self.worker is not None:
                    self.worker.join(timeout=5)
                    if self.worker.is_alive():raise ExecutionError("SDK launch worker was not reaped")
                    if self.bridge is None:
                        raise ExecutionError("partial SDK launch has no confirmed output bridge settlement")
                if self.worker_error:raise ExecutionError("SDK launch worker failed")
        except BaseException:
            self.close_error = True
            raise
