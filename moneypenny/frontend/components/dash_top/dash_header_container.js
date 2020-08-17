import {
  connect
} from 'react-redux';
import DashHeader from './dash_header';
import {
  logout
} from '../../actions/session_actions';

const mapStateToProps = () => ({

})

const mapDispatchToProps = (dispatch) => ({
  logout: () => dispatch(logout())
})



export default connect(mapStateToProps, mapDispatchToProps)(DashHeader);