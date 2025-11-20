import React from 'react';
import { useHistory } from 'react-router';

const OAuthRedirect: React.FC = () => {
  const history = useHistory();

  // Since we're using local CSV data, redirect to demo stats page instead
  React.useEffect(() => {
    history.push('/stats/demo');
  }, [history]);

  return (
    <div style={{ textAlign: 'center', fontSize: 20 }}>
      Redirecting to demo user stats (using pre-loaded CSV data)...
    </div>
  );
};

export default OAuthRedirect;
