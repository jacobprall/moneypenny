import {
  connect
} from 'react-redux';
import DashHeader from './dash_header';
import {
  logout
} from '../../actions/session_actions';
import { openModal } from '../../actions/modal_actions'
import AccountFormContainer from '../accounts/account_form_modals/account_form_container'


const mapStateToProps = (state) => ({
   errors: Object.values(state.errors.account),
     formType: 'new',
     passedAccount: {
       'account_category': 'Cash',
       'balance': 0,
       'debit': true,
       'institution': "None",
       'label': "",
       'user_id': state.session.id
     },
     AccountFormContainer
})

const mapDispatchToProps = (dispatch) => ({
  logout: () => dispatch(logout()),
  openModal: (formType, component, payload) => dispatch(openModal(formType, component, payload))

})



export default connect(mapStateToProps, mapDispatchToProps)(DashHeader);