
// This template was generated from the notebook: https://dev.fiberplane.io/notebook/Incident-1234-High-error-rate-on-MyService-example-incident-resolution-v1--Ddg-NYR6ROyfLJLDasFz-A

local fp = import 'fiberplane.libsonnet';

function(
   // Any arguments to this function will be filled
   // in with values when the template is evaluated.
   //
   // You can replace fixed values anywhere in the 
   // template with the argument names and the
   // values will be substituted accordingly.

   title='Incident #1234 - High error rate on MyService (example incident resolution v1)'
)
  fp.notebook.new(title)
    .setTimeRangeRelative(480)
    .addOrganizationDataSource(
      alias='default',
      id='djGaQKjySL-TONwxtk_gjQ',
      name='default',
      dataSource=fp.dataSource.prometheus(
        url='https://prometheus.dev.fiberplane.io', 
      ),
      defaultDataSource=true,
    )
    .addCells([
      fp.cell.heading.h1(
        content='Overview',
        readOnly=false,
      ),
      fp.cell.heading.h2(
        content='Service Overview',
        readOnly=false,
      ),
      fp.cell.text(
        content="MyService is a web application that provides cat gifs upon request. Users access the website and click 'give me cats' and a cat gif is displayed.",
        readOnly=false,
      ),
      fp.cell.heading.h2(
        content='Incident Overview ',
        readOnly=false,
      ),
      fp.cell.divider(
        readOnly=false,
      ),
      fp.cell.text(
        content='Received an alert for high error rate on MyService. The ALB is returning 503s to the user. See screenshot. This is impacting all users.',
        readOnly=false,
      ),
      fp.cell.prometheus(
        content='container_network_receive_errors_total',
        readOnly=false,
      ),
      fp.cell.text(
        content='Customer sees the following:',
        readOnly=false,
      ),
      fp.cell.heading.h2(
        content='Incident Analysis',
        readOnly=false,
      ),
      fp.cell.heading.h3(
        content='Outstanding actions',
        readOnly=false,
      ),
      fp.cell.checkbox(
        checked=true,
        content='Fiona to check API requests are generating calls to the DB - graph shared in notebook ',
        level=null,
        readOnly=false,
      ),
      fp.cell.checkbox(
        checked=false,
        content='Bob to check DB is receiving calls from API and running the queries successfully ',
        level=null,
        readOnly=false,
      ),
      fp.cell.text(
        content='',
        readOnly=false,
      ),
      fp.cell.divider(
        readOnly=false,
      ),
      fp.cell.heading.h3(
        content='Hypothesis 1 is that...',
        readOnly=false,
      ),
      fp.cell.text(
        content='The lambda functions are unavailable so returning 503s to the customer',
        readOnly=false,
      ),
      fp.cell.text(
        content='**we can disprove this by** ',
        readOnly=false,
      ),
      fp.cell.text(
        content='ensuring that we can see our lambda functions processing requests and sending requests to the DB',
        readOnly=false,
      ),
      fp.cell.prometheus(
        content='apiserver_request_total',
        readOnly=false,
      ),
      fp.cell.heading.h3(
        content='Conclusion',
        readOnly=false,
      ),
      fp.cell.text(
        content='The API is processing requests successfully as we can see requests made to the DB',
        readOnly=false,
      ),
      fp.cell.text(
        content='',
        readOnly=false,
      ),
      fp.cell.divider(
        readOnly=false,
      ),
      fp.cell.heading.h3(
        content='Hypothesis 2 is that....',
        readOnly=false,
      ),
      fp.cell.text(
        content="The database isn't responding to queries, ",
        readOnly=false,
      ),
      fp.cell.text(
        content='**we can disprove this by** ',
        readOnly=false,
      ),
      fp.cell.text(
        content='seeing successful responses being sent from the DB back to the API',
        readOnly=false,
      ),
      fp.cell.code(
        content='this is some code',
        syntax=null,
        readOnly=false,
      ),
      fp.cell.code(
        content='',
        syntax=null,
        readOnly=false,
      ),
      fp.cell.divider(
        readOnly=false,
      ),
    ])
