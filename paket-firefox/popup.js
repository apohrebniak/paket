document.addEventListener('DOMContentLoaded', function() {
  const endpointInput = document.getElementById('endpoint');
  const headersInput = document.getElementById('headers');
  const statusDiv = document.getElementById('status');
  
  let currentUrl = '';
  
  // Load saved configuration and get current URL
  Promise.all([
    browser.storage.sync.get(['endpoint', 'headers']),
    browser.tabs.query({active: true, currentWindow: true})
  ]).then(function([config, tabs]) {
    // Set configuration
    if (config.endpoint) {
      endpointInput.value = config.endpoint;
    }
    if (config.headers) {
      headersInput.value = config.headers;
    }
    
    // Get current URL
    currentUrl = tabs[0].url;
    
    // Automatically send request
    autoSendUrl(config);
  });
  
  // Save configuration when changed
  endpointInput.addEventListener('input', function() {
    browser.storage.sync.set({endpoint: endpointInput.value});
  });
  
  headersInput.addEventListener('input', function() {
    browser.storage.sync.set({headers: headersInput.value});
  });
  
  function autoSendUrl(config) {
    const endpoint = config.endpoint || endpointInput.value.trim();
    
    if (!endpoint) {
      showStatus('⚠ Please configure Paket endpoint', 'error');
      return;
    }
    
    if (!currentUrl) {
      showStatus('✗ Could not get current URL', 'error');
      return;
    }
    
    // Parse custom headers
    let customHeaders = {};
    const headersValue = config.headers || headersInput.value.trim();
    if (headersValue) {
      try {
        customHeaders = JSON.parse(headersValue);
      } catch (e) {
        showStatus('✗ Invalid JSON format for headers', 'error');
        return;
      }
    }
    
    sendUrlToServer(endpoint, currentUrl, customHeaders);
  }
  
  function sendUrlToServer(endpoint, url, customHeaders) {
    // Prepare URL-encoded form data
    const formData = new URLSearchParams();
    formData.append('url', url);
    
    // Prepare headers with correct content type
    const headers = {
      'Content-Type': 'application/x-www-form-urlencoded',
      ...customHeaders
    };
    
    fetch(endpoint, {
      method: 'PUT',
      headers: headers,
      body: formData
    })
    .then(function(response) {
      if (response.ok) {
        showStatus('✓ Success! URL saved', 'success');
      } else {
        showStatus('✗ Server error: ' + response.status, 'error');
      }
    })
    .catch(function(error) {
      showStatus('✗ Network error: ' + error.message, 'error');
    });
  }
  
  function showStatus(message, type) {
    statusDiv.textContent = message;
    statusDiv.className = 'status ' + type;
  }
});

