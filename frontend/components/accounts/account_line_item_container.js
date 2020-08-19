import {
  connect
} from 'react-redux';
import AccountLineItem from './account_line_item';
import {
  openModal
} from '../../actions/modal_actions'
const mapStateToProps = (state, ownProps) => ({
  account: ownProps.account
})

const mapDispatchToProps = (dispatch) => ({
  openModal: modalType => dispatch(openModal(modalType))

})



export default connect(mapStateToProps, mapDispatchToProps)(AccountLineItem);