module.exports = {
  'to be [FAILED]' : function(test) {},
  'to be [FAILED]' : function() {},
  'to be with message [PASSED]' : function(test) {
    var expect = this.client.api.expect.element('#weblogin').to.be.an('input', 'weblogin should be an input');
    this.client.on('nightwatch:finished', function(results, errors) {})
  },
}
