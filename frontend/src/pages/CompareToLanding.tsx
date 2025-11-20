import React from 'react';
import { Link } from 'react-router-dom';

import { useUsername } from 'src/store/selectors';
import { colors } from 'src/style';
import './CompareToLanding.scss';
import '../components/BigButton.scss';

const CompareToLanding: React.FC = () => {
  const { username, displayName } = useUsername();

  return (
    <div className="compare-to-landing">
      <Link to="/" style={{ textDecorationColor: '#ccc' }}>
        <div className="compare-to-landing-header">
          <img
            src="/spotifytrack-smaller.jpg"
            style={{ height: 34, width: 34, marginRight: 6, marginLeft: -4, marginTop: -2 }}
            alt="spotifytrack logo"
          />
          <h2>Spotifytrack</h2>
        </div>
      </Link>
      <h1>
        Music Taste Comparison{' '}
        <span style={{ color: colors.pink, whiteSpace: 'nowrap' }}>(Demo Mode)</span>
      </h1>

      <div className="content-embed">
        <p>
          This is a demo version using pre-loaded listening data from a CSV file.
          The comparison feature is not available in demo mode as it requires personal Spotify data.
        </p>
        <p>
          <strong>Note:</strong> This demo uses static data and does not connect to Spotify.
        </p>

        <div style={{ textAlign: 'center' }}>
          <Link to="/stats/demo">
            <button className="big-button">View Demo Stats</button>
          </Link>
        </div>
      </div>
    </div>
  );
};

export default CompareToLanding;
