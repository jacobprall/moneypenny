import {
  connect
} from 'react-redux';
import DashHeader from './dash_header';
import {
  logout
} from '../../actions/session_actions';
import { openModal } from '../../actions/modal_actions'
const mapStateToProps = () => ({

})

const mapDispatchToProps = (dispatch) => ({
  logout: () => dispatch(logout()),
  openModal: modalType => dispatch(openModal(modalType))

})



export default connect(mapStateToProps, mapDispatchToProps)(DashHeader);