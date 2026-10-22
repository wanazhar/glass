#!/usr/bin/env python3
"""Local HTTPS regression for quiet reads and cancellation; run cargo test first; requires rustc and openssl."""
import tempfile, subprocess, pathlib, http.server, ssl, threading, time
root=pathlib.Path(__file__).resolve().parents[3]
class Server(http.server.ThreadingHTTPServer): daemon_threads=True
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*_): pass
 def do_GET(self):
  self.send_response(200); self.send_header('Content-Length','1'); self.end_headers(); self.wfile.flush()
  if self.path=='/quiet':
   time.sleep(.45); self.wfile.write(b'X'); self.wfile.flush()
  else:
   self.connection.settimeout(3)
   try: self.connection.recv(1)
   except (OSError, ssl.SSLError): pass
with tempfile.TemporaryDirectory(prefix='gc-tls-check-') as tmp:
 p=pathlib.Path(tmp)
 subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(p/'key.pem'),'-out',str(p/'cert.pem'),'-subj','/CN=localhost','-days','1','-addext','subjectAltName=DNS:localhost','-addext','basicConstraints=critical,CA:FALSE'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 source='''#[path="__CANCELLABLE_RS__"] mod cancellable;
use std::{io::Read,sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
fn main(){
 let args:Vec<_>=std::env::args().collect();
 let cert=ureq::tls::Certificate::from_pem(&std::fs::read(&args[2]).unwrap()).unwrap();
 let config=ureq::config::Config::builder().timeout_global(Some(Duration::from_secs(5))).tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::Specific(Arc::new(vec![cert]))).build()).build();
 let cancel=Arc::new(AtomicBool::new(false)); let flag=cancel.clone();
 if args[3]=="cancel" {std::thread::spawn(move||{std::thread::sleep(Duration::from_millis(750));flag.store(true,Ordering::Release);});}
 let started=Instant::now();
 let result=cancellable::scoped(cancel,||{let mut response=cancellable::agent(config).get(&args[1]).call().map_err(|e|e.to_string())?;let mut body=String::new();response.body_mut().as_reader().read_to_string(&mut body).map_err(|e|e.to_string())?;Ok::<_,String>(body)});
 if args[3]=="cancel" {assert!(result.unwrap_err().contains("cancelled"));assert!(started.elapsed()<Duration::from_secs(2));}else{assert_eq!(result.unwrap(),"X");}
 println!("PASS TLS {}: {:?}",args[3],started.elapsed());
}'''
 source=source.replace('__CANCELLABLE_RS__', str(root/'crates/glass-core/src/provider/cancellable.rs'))
 (p/'main.rs').write_text(source)
 target=pathlib.Path(__import__('os').environ.get('CARGO_TARGET_DIR',root/'target'))
 lib=next((target/'debug/deps').glob('libureq-*.rlib'))
 native=next((target/'debug/build').glob('ring-*/out/libring_core_0_17_14_.a')).parent
 subprocess.run(['rustc','--edition=2024',str(p/'main.rs'),'-o',str(p/'client'),'-L',f'dependency={target}/debug/deps','-L',f'native={native}','--extern',f'ureq={lib}'],check=True)
 server=Server(('127.0.0.1',0),Handler)
 context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(p/'cert.pem',p/'key.pem');server.socket=context.wrap_socket(server.socket,server_side=True)
 threading.Thread(target=server.serve_forever,daemon=True).start()
 try:
  for mode in ['quiet','cancel']:
   subprocess.run([str(p/'client'),f'https://localhost:{server.server_port}/{mode}',str(p/'cert.pem'),mode],check=True,timeout=6)
 finally: server.shutdown()
