const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:3002/ws/agent');

ws.on('open', () => {
  console.log('WebSocket connected');
  
  // Send auth message
  const auth = {
    type: 'auth',
    deviceToken: 'test-token',
    agentId: 'test-agent',
    zone: 'test-zone',
    branchId: 'test-branch'
  };
  ws.send(JSON.stringify(auth));
  console.log('Auth message sent');
});

ws.on('message', (data) => {
  console.log('Received:', data.toString());
});

ws.on('error', (error) => {
  console.error('WebSocket error:', error.message);
});

ws.on('close', () => {
  console.log('WebSocket closed');
});
